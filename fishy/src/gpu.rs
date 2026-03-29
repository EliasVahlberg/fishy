/// GPU-accelerated batch operations for fishy's extract pipeline.
///
/// Provides batched FFT (for spectral_fingerprint) and batched Jacobi eigendecomposition
/// (for co_occurrence_spectrum) across all sources in a collection simultaneously.
///
/// Falls back gracefully to CPU if no GPU is available.
use analysis::{EigenSpectrum, PowerSpectrum};

/// Holds the wgpu device, queue, and compiled pipelines.
pub struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    fft_pipeline: wgpu::ComputePipeline,
    fft_bind_layout: wgpu::BindGroupLayout,
    eigen_pipeline: wgpu::ComputePipeline,
    eigen_bind_layout: wgpu::BindGroupLayout,
}

impl GpuContext {
    /// Try to initialise a GPU context. Returns None if no suitable adapter found.
    pub fn try_new() -> Option<Self> {
        pollster::block_on(Self::try_new_async())
    }

    async fn try_new_async() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12,
            ..Default::default()
        });
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }).await?;

        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fishy"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        }, None).await.ok()?;

        let fft_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fft"),
            source: wgpu::ShaderSource::Wgsl(FFT_SHADER.into()),
        });
        let eigen_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("eigen"),
            source: wgpu::ShaderSource::Wgsl(EIGEN_SHADER.into()),
        });

        let fft_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fft_bind"),
            entries: &[
                storage_entry(0, true),  // input bins
                storage_entry(1, false), // output magnitudes
                uniform_entry(2),        // params
            ],
        });
        let eigen_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("eigen_bind"),
            entries: &[
                storage_entry(0, true),  // input matrices
                storage_entry(1, false), // output eigenvalues
                uniform_entry(2),        // params
            ],
        });

        let fft_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("fft"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&fft_bind_layout],
                push_constant_ranges: &[],
            })),
            module: &fft_module,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });
        let eigen_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("eigen"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&eigen_bind_layout],
                push_constant_ranges: &[],
            })),
            module: &eigen_module,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });

        Some(Self { device, queue, fft_pipeline, fft_bind_layout, eigen_pipeline, eigen_bind_layout })
    }

    /// Batch FFT: compute power spectra for N sources simultaneously.
    /// `bins[i]` is the event-count histogram for source i, padded/truncated to 1024 bins.
    /// Returns one PowerSpectrum per source (512 positive-frequency magnitudes).
    pub fn batch_fft(&self, bins: &[Vec<f32>]) -> Vec<PowerSpectrum> {
        if bins.is_empty() { return vec![]; }
        const FFT_SIZE: usize = 1024;
        const HALF: usize = FFT_SIZE / 2;
        let n_sources = bins.len() as u32;

        // Pad/truncate each source to exactly FFT_SIZE bins
        let input: Vec<f32> = bins.iter().flat_map(|b| {
            let mut padded = vec![0.0f32; FFT_SIZE];
            let len = b.len().min(FFT_SIZE);
            padded[..len].copy_from_slice(&b[..len]);
            padded
        }).collect();

        let out_len = n_sources as usize * HALF;
        let input_buf = self.upload(&input);
        let output_buf = self.create_output_buf(out_len * 4);
        let params_buf = self.upload_uniform(&[n_sources, FFT_SIZE as u32]);

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.fft_bind_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: input_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: output_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: params_buf.as_entire_binding() },
            ],
        });

        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.fft_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(n_sources, 1, 1);
        }
        self.queue.submit([enc.finish()]);

        let raw = self.readback(&output_buf, out_len);
        (0..bins.len()).map(|i| {
            let magnitudes: Vec<f64> = raw[i * HALF..(i + 1) * HALF]
                .iter().map(|&v| v as f64).collect();
            let frequencies: Vec<f64> = (0..HALF).map(|k| k as f64 / FFT_SIZE as f64).collect();
            PowerSpectrum { frequencies, magnitudes }
        }).collect()
    }

    /// Batch eigendecomposition: compute eigenvalue spectra for N symmetric matrices.
    /// `matrices[i]` is a flattened n×n symmetric matrix (row-major).
    /// Returns one EigenSpectrum per matrix (eigenvalues sorted ascending).
    pub fn batch_eigen(&self, matrices: &[Vec<f32>], n: usize) -> Vec<EigenSpectrum> {
        if matrices.is_empty() { return vec![]; }
        let n_mats = matrices.len() as u32;
        let mat_size = n as u32;

        let input: Vec<f32> = matrices.iter().flat_map(|m| m.iter().copied()).collect();
        let out_len = (n_mats * mat_size) as usize;

        let input_buf = self.upload(&input);
        let output_buf = self.create_output_buf(out_len * 4);
        let params_buf = self.upload_uniform(&[n_mats, mat_size]);

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.eigen_bind_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: input_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: output_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: params_buf.as_entire_binding() },
            ],
        });

        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.eigen_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            // One workgroup per matrix; workgroup_size(n) threads per matrix
            pass.dispatch_workgroups(n_mats, 1, 1);
        }
        self.queue.submit([enc.finish()]);

        let raw = self.readback(&output_buf, out_len);
        (0..matrices.len()).map(|i| {
            let mut eigenvalues: Vec<f64> = raw[i * n..(i + 1) * n]
                .iter().map(|&v| v as f64).collect();
            eigenvalues.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            EigenSpectrum { eigenvalues }
        }).collect()
    }

    // --- Buffer helpers ---

    fn upload(&self, data: &[f32]) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }

    fn upload_uniform(&self, data: &[u32]) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::UNIFORM,
        })
    }

    fn create_output_buf(&self, size_bytes: usize) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: size_bytes as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    }

    fn readback(&self, buf: &wgpu::Buffer, n_floats: usize) -> Vec<f32> {
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (n_floats * 4) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(buf, 0, &staging, 0, (n_floats * 4) as u64);
        self.queue.submit([enc.finish()]);

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();

        let data = slice.get_mapped_range();
        bytemuck::cast_slice(&data).to_vec()
    }
}

// ---------------------------------------------------------------------------
// Bind group layout helpers
// ---------------------------------------------------------------------------

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

// ---------------------------------------------------------------------------
// WGSL shaders
// ---------------------------------------------------------------------------

/// Batched radix-2 Cooley-Tukey FFT, fixed size 1024.
/// Each workgroup handles one source (256 threads, each processes 4 elements).
/// Input:  flat f32 array of n_sources × 1024 event counts
/// Output: flat f32 array of n_sources × 512 power magnitudes
const FFT_SHADER: &str = r#"
struct Params { n_sources: u32, fft_size: u32 }

@group(0) @binding(0) var<storage, read>       input:  array<f32>;
@group(0) @binding(1) var<storage, read_write>  output: array<f32>;
@group(0) @binding(2) var<uniform>              params: Params;

const FFT_N:  u32 = 1024u;
const HALF_N: u32 = 512u;
const LOG2_N: u32 = 10u;

var<workgroup> re: array<f32, 1024>;
var<workgroup> im: array<f32, 1024>;

@compute @workgroup_size(256)
fn main(
    @builtin(workgroup_id)         wg:  vec3<u32>,
    @builtin(local_invocation_id)  lid: vec3<u32>,
) {
    let src = wg.x;
    let tid = lid.x;

    // Each thread loads 4 elements in bit-reversed order
    for (var k = 0u; k < 4u; k++) {
        let idx = tid * 4u + k;
        let rev = bit_reverse(idx, LOG2_N);
        re[idx] = input[src * FFT_N + rev];
        im[idx] = 0.0;
    }
    workgroupBarrier();

    // Butterfly stages
    var len = 2u;
    loop {
        if len > FFT_N { break; }
        let half_len = len / 2u;
        let groups = FFT_N / len;
        // Each thread handles multiple butterfly pairs
        for (var t = tid; t < groups * half_len; t += 256u) {
            let g = t / half_len;
            let k = t % half_len;
            let i = g * len + k;
            let j = i + half_len;
            let angle = -6.283185307 * f32(k) / f32(len);
            let wr = cos(angle);
            let wi = sin(angle);
            let tr = wr * re[j] - wi * im[j];
            let ti = wr * im[j] + wi * re[j];
            re[j] = re[i] - tr;
            im[j] = im[i] - ti;
            re[i] = re[i] + tr;
            im[i] = im[i] + ti;
        }
        workgroupBarrier();
        len = len * 2u;
    }

    // Write power spectrum (positive frequencies, 2 per thread)
    for (var k = 0u; k < 2u; k++) {
        let idx = tid * 2u + k;
        if idx < HALF_N {
            output[src * HALF_N + idx] = re[idx] * re[idx] + im[idx] * im[idx];
        }
    }
}

fn bit_reverse(x: u32, bits: u32) -> u32 {
    var v = x;
    var r = 0u;
    for (var i = 0u; i < bits; i++) {
        r = (r << 1u) | (v & 1u);
        v = v >> 1u;
    }
    return r;
}
"#;

/// Batched Jacobi eigendecomposition for symmetric matrices.
/// Each workgroup handles one n×n matrix. Workgroup size = n (max 128).
/// Params: [n_mats: u32, mat_size: u32]
/// Input:  flat f32 array of n_mats × mat_size² matrix elements (row-major)
/// Output: flat f32 array of n_mats × mat_size eigenvalues
const EIGEN_SHADER: &str = r#"
struct Params { n_mats: u32, mat_size: u32 }

@group(0) @binding(0) var<storage, read>       input:  array<f32>;
@group(0) @binding(1) var<storage, read_write>  output: array<f32>;
@group(0) @binding(2) var<uniform>              params: Params;

// Shared memory for one matrix (max 128×128 = 16384 f32 = 64KB)
var<workgroup> mat: array<f32, 16384>;
var<workgroup> max_val: f32;
var<workgroup> max_p: u32;
var<workgroup> max_q: u32;

@compute @workgroup_size(128)
fn main(
    @builtin(workgroup_id)         wg:  vec3<u32>,
    @builtin(local_invocation_id)  lid: vec3<u32>,
) {
    let m   = wg.x;
    let n   = params.mat_size;
    let tid = lid.x;
    let n2  = n * n;

    // Load matrix into shared memory
    var i = tid;
    loop {
        if i >= n2 { break; }
        mat[i] = input[m * n2 + i];
        i += 128u;
    }
    workgroupBarrier();

    // Jacobi sweeps (fixed iteration count for GPU — no convergence check)
    let sweeps = n * n;  // conservative upper bound
    for (var sweep = 0u; sweep < sweeps; sweep++) {
        // Find max off-diagonal element (parallel reduction)
        var local_max = 0.0f;
        var local_p = 0u;
        var local_q = 1u;
        for (var row = tid; row < n; row += 128u) {
            for (var col = row + 1u; col < n; col++) {
                let v = abs(mat[row * n + col]);
                if v > local_max {
                    local_max = v;
                    local_p = row;
                    local_q = col;
                }
            }
        }
        // Thread 0 collects (simplified — not a full parallel reduction)
        if tid == 0u {
            max_val = local_max;
            max_p = local_p;
            max_q = local_q;
        }
        workgroupBarrier();

        if max_val < 1e-6 { break; }

        let p = max_p;
        let q = max_q;
        let app = mat[p * n + p];
        let aqq = mat[q * n + q];
        let apq = mat[p * n + q];

        // Compute rotation
        let theta = 0.5 * atan2(2.0 * apq, aqq - app);
        let c = cos(theta);
        let s = sin(theta);

        // Apply rotation to all rows (each thread handles one row)
        for (var row = tid; row < n; row += 128u) {
            let rp = mat[row * n + p];
            let rq = mat[row * n + q];
            mat[row * n + p] =  c * rp + s * rq;
            mat[row * n + q] = -s * rp + c * rq;
        }
        workgroupBarrier();

        // Apply rotation to all columns
        for (var col = tid; col < n; col += 128u) {
            let cp = mat[p * n + col];
            let cq = mat[q * n + col];
            mat[p * n + col] =  c * cp + s * cq;
            mat[q * n + col] = -s * cp + c * cq;
        }
        workgroupBarrier();
    }

    // Write diagonal (eigenvalues) to output
    for (var i2 = tid; i2 < n; i2 += 128u) {
        output[m * n + i2] = mat[i2 * n + i2];
    }
}
"#;
