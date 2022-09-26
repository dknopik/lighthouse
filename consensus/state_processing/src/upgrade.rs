pub mod altair;
pub mod merge;
pub mod eip4844;

pub use altair::upgrade_to_altair;
pub use merge::upgrade_to_bellatrix;
pub use eip4844::upgrade_to_eip4844;
