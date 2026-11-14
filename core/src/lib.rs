//! Priogator's data side, without any UI: the model, the board and its
//! operations, recurring tasks, active hours, the sync folder files,
//! settings and backups. The desktop app (egui) builds on it; other front
//! ends can too, and then read and write the same files the same way.

pub mod active;
pub mod backup;
pub mod board;
pub mod demo;
pub mod model;
pub mod repeat;
pub mod storage;
