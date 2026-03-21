use crate::types::{PowerSpectrum, WaveletCoefficients};

/// FFT power spectrum of an event rate time series.
///
/// Bins events into fixed-width buckets, then computes the DFT power spectrum
/// using the Cooley-Tukey radix-2 algorithm on the next power-of-two length.
pub fn spectral_fingerprint(event_times: &[u64], bin_width: u64) -> PowerSpectrum {
    if event_times.is_empty() || bin_width == 0 {
        return PowerSpectrum { frequencies: vec![], magnitudes: vec![] };
    }

    let max_t = event_times.iter().copied().max().unwrap_or(0);
    let n_bins = ((max_t / bin_width) + 1) as usize;
    let mut bins = vec![0.0f64; n_bins];
    for &t in event_times {
        bins[(t / bin_width) as usize] += 1.0;
    }

    // Zero-pad to next power of two.
    let fft_len = n_bins.next_power_of_two();
    bins.resize(fft_len, 0.0);

    let spectrum = fft_power(&bins);
    // Only keep the positive-frequency half.
    let half = fft_len / 2;
    let magnitudes = spectrum[..half].to_vec();
    let frequencies: Vec<f64> = (0..half)
        .map(|k| k as f64 / (fft_len as f64 * bin_width as f64))
        .collect();

    PowerSpectrum { frequencies, magnitudes }
}

/// JSD between two power spectra (treated as probability distributions).
pub fn spectral_divergence(baseline: &PowerSpectrum, test: &PowerSpectrum) -> f64 {
    if baseline.magnitudes.is_empty() || test.magnitudes.is_empty() {
        return 0.0;
    }

    // Align to the shorter length.
    let len = baseline.magnitudes.len().min(test.magnitudes.len());
    let b_total: f64 = baseline.magnitudes[..len].iter().sum();
    let t_total: f64 = test.magnitudes[..len].iter().sum();
    if b_total < 1e-10 && t_total < 1e-10 {
        return 0.0;
    }

    let (mut kl_bm, mut kl_tm) = (0.0f64, 0.0f64);
    for i in 0..len {
        let p = baseline.magnitudes[i] / (b_total + 1e-10);
        let q = test.magnitudes[i] / (t_total + 1e-10);
        let m = 0.5 * (p + q);
        if p > 0.0 { kl_bm += p * (p / (m + 1e-10)).ln(); }
        if q > 0.0 { kl_tm += q * (q / (m + 1e-10)).ln(); }
    }

    (0.5 * (kl_bm + kl_tm) / std::f64::consts::LN_2).clamp(0.0, 1.0)
}

/// Haar wavelet decomposition of an event rate time series.
pub fn wavelet_decompose(event_times: &[u64], bin_width: u64, levels: usize) -> WaveletCoefficients {
    if event_times.is_empty() || bin_width == 0 || levels == 0 {
        return WaveletCoefficients { levels: vec![] };
    }

    let max_t = event_times.iter().copied().max().unwrap_or(0);
    let n_bins = ((max_t / bin_width) + 1) as usize;
    let mut signal = vec![0.0f64; n_bins.next_power_of_two()];
    for &t in event_times {
        signal[(t / bin_width) as usize] += 1.0;
    }

    let mut result = Vec::with_capacity(levels);
    for _ in 0..levels {
        if signal.len() < 2 {
            break;
        }
        let mut approx = Vec::with_capacity(signal.len() / 2);
        let mut detail = Vec::with_capacity(signal.len() / 2);
        for chunk in signal.chunks_exact(2) {
            approx.push((chunk[0] + chunk[1]) * std::f64::consts::FRAC_1_SQRT_2);
            detail.push((chunk[0] - chunk[1]) * std::f64::consts::FRAC_1_SQRT_2);
        }
        result.push(detail);
        signal = approx;
    }
    // Append final approximation as the coarsest level.
    result.push(signal);
    result.reverse(); // coarse to fine

    WaveletCoefficients { levels: result }
}

// ---------------------------------------------------------------------------
// Cooley-Tukey radix-2 DFT (in-place, power-of-two length)
// ---------------------------------------------------------------------------

fn fft_power(signal: &[f64]) -> Vec<f64> {
    let n = signal.len();
    let mut re: Vec<f64> = signal.to_vec();
    let mut im: Vec<f64> = vec![0.0; n];

    // Bit-reversal permutation.
    let bits = n.trailing_zeros() as usize;
    for i in 0..n {
        let j = bit_reverse(i, bits);
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterfly stages.
    let mut len = 2usize;
    while len <= n {
        let half = len / 2;
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        for i in (0..n).step_by(len) {
            for k in 0..half {
                let wr = (ang * k as f64).cos();
                let wi = (ang * k as f64).sin();
                let tr = wr * re[i + k + half] - wi * im[i + k + half];
                let ti = wr * im[i + k + half] + wi * re[i + k + half];
                re[i + k + half] = re[i + k] - tr;
                im[i + k + half] = im[i + k] - ti;
                re[i + k] += tr;
                im[i + k] += ti;
            }
        }
        len *= 2;
    }

    re.iter().zip(im.iter()).map(|(r, i)| r * r + i * i).collect()
}

fn bit_reverse(mut x: usize, bits: usize) -> usize {
    let mut result = 0;
    for _ in 0..bits {
        result = (result << 1) | (x & 1);
        x >>= 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_spectra_zero_divergence() {
        let times: Vec<u64> = (0..100).map(|i| i * 10).collect();
        let s = spectral_fingerprint(&times, 10);
        assert_eq!(spectral_divergence(&s, &s), 0.0);
    }

    #[test]
    fn periodic_signal_has_peak() {
        // Events every 100 units → strong peak at f = 0.01.
        let times: Vec<u64> = (0..64).map(|i| i * 100).collect();
        let s = spectral_fingerprint(&times, 10);
        let max_mag = s.magnitudes.iter().cloned().fold(0.0f64, f64::max);
        assert!(max_mag > 0.0);
    }

    #[test]
    fn wavelet_levels_count() {
        let times: Vec<u64> = (0..64).map(|i| i * 10).collect();
        let w = wavelet_decompose(&times, 10, 3);
        assert_eq!(w.levels.len(), 4); // 3 detail levels + 1 approximation
    }
}
