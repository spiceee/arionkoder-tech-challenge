# arionkoder-tech-challenge

## 5. Simple Task Scheduler

Build a basic Rust-based scheduler that:
- Manages task execution using threads.
- Supports task prioritization and simple dependency handling.

Constraints:
- Gracefully handle thread panics and errors.
- Allow simple monitoring of task completion status.

## Caveats
- Tasks are represented as closures.
- Dependencies are represented as a list of task IDs.
- Dependencies are a mere check for a completion status.
