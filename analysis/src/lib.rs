//! Stateless analysis functions for information-fusion anomaly detection.
//!
//! Flat API — each function takes data in, returns data out. No orchestration,
//! no strategy selection, no domain knowledge. Pure math.

mod types;

mod co_occurrence;
mod dependency;
mod distributional;
mod ds;
mod entropy;
mod evidence;
mod spectral;

pub use types::*;

// --- Distributional ---

pub use distributional::distributional_divergence;

// --- Dependency ---

pub use dependency::{mi_matrix_divergence, mutual_information_matrix};

// --- Spectral ---

pub use spectral::{spectral_divergence, spectral_fingerprint, wavelet_decompose};

// --- Evidence ---

pub use evidence::evidence_bpa;

// --- Co-occurrence ---

pub use co_occurrence::co_occurrence_spectrum;

// --- Dempster-Shafer ---

pub use ds::{ds_combine, ds_combine_many, ds_conflict};

// --- Entropy ---

pub use entropy::{matrix_entropy, shannon_entropy, spectral_entropy};
