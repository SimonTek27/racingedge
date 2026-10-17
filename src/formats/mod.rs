//! RacingEdge format readers/writers.
//!
//! Each submodule parses one binary or textual format used by the game
//! content. The formats are shared by the game binary and the CLI
//! (AGENTS.md, Rule 3).

pub mod ai_spline;
pub mod kn5;
pub mod ksanim;
