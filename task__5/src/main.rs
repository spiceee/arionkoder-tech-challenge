#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Blocked,
}

#[derive(Clone)]
pub struct Task {
    pub id: u64,
    pub name: String,
    pub priority: u8, // Higher number = higher priority
    pub dependencies: Vec<u64>,
    pub function:
        Arc<dyn Fn() -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync>,
}

impl Task {
    pub fn new<F>(id: u64, name: String, priority: u8, function: F) -> Self
    where
        F: Fn() -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static,
    {
        Self {
            id,
            name,
            priority,
            dependencies: Vec::new(),
            function: Arc::new(function),
        }
    }

    #[must_use]
    pub fn with_dependencies(mut self, dependencies: Vec<u64>) -> Self {
        self.dependencies = dependencies;
        self
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Task {}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
    }
}

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: u64,
    pub status: TaskStatus,
    pub error: Option<String>,
}

pub struct Scheduler {
    tasks: Arc<Mutex<BinaryHeap<Task>>>,
    task_status: Arc<Mutex<HashMap<u64, TaskStatus>>>,
    completed_tasks: Arc<Mutex<HashSet<u64>>>,
    failed_tasks: Arc<Mutex<HashSet<u64>>>,
    results: Arc<Mutex<Vec<TaskResult>>>,
    worker_count: usize,
    shutdown: Arc<Mutex<bool>>,
    condvar: Arc<Condvar>,
}

impl Scheduler {
    #[must_use]
    pub fn new(worker_count: usize) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(BinaryHeap::new())),
            task_status: Arc::new(Mutex::new(HashMap::new())),
            completed_tasks: Arc::new(Mutex::new(HashSet::new())),
            failed_tasks: Arc::new(Mutex::new(HashSet::new())),
            results: Arc::new(Mutex::new(Vec::new())),
            worker_count,
            shutdown: Arc::new(Mutex::new(false)),
            condvar: Arc::new(Condvar::new()),
        }
    }

    /// # Panics
    /// Panics if the mutex is poisoned.
    pub fn add_task(&self, task: Task) {
        self.task_status
            .lock()
            .unwrap()
            .insert(task.id, TaskStatus::Pending);
        self.tasks.lock().unwrap().push(task);
        self.condvar.notify_one();
    }

    /// # Panics
    /// Panics if any mutex is poisoned or if condvar operations fail.
    #[must_use]
    pub fn run_worker(
        worker_id: usize,
        tasks: Arc<Mutex<BinaryHeap<Task>>>,
        task_status: Arc<Mutex<HashMap<u64, TaskStatus>>>,
        completed_tasks: Arc<Mutex<HashSet<u64>>>,
        failed_tasks: Arc<Mutex<HashSet<u64>>>,
        results: Arc<Mutex<Vec<TaskResult>>>,
        shutdown: Arc<Mutex<bool>>,
        condvar: Arc<Condvar>,
    ) {
        loop {
            let task = Self::get_next_available_task(
                &tasks,
                &task_status,
                &completed_tasks,
                &failed_tasks,
                &results,
                &shutdown,
                &condvar,
            );

            if let Some(task) = task {
                Self::execute_task(
                    worker_id,
                    task,
                    &task_status,
                    &completed_tasks,
                    &failed_tasks,
                    &results,
                    &condvar,
                );
            } else if *shutdown.lock().unwrap() {
                break;
            } else {
                // No available tasks, wait a bit
                thread::sleep(Duration::from_millis(10));
            }
        }

        println!("Worker {worker_id} shutting down");
    }

    fn get_next_available_task(
        tasks: &Arc<Mutex<BinaryHeap<Task>>>,
        task_status: &Arc<Mutex<HashMap<u64, TaskStatus>>>,
        completed_tasks: &Arc<Mutex<HashSet<u64>>>,
        failed_tasks: &Arc<Mutex<HashSet<u64>>>,
        results: &Arc<Mutex<Vec<TaskResult>>>,
        shutdown: &Arc<Mutex<bool>>,
        condvar: &Arc<Condvar>,
    ) -> Option<Task> {
        let mut tasks_guard = tasks.lock().unwrap();

        // Wait for tasks or shutdown signal
        while tasks_guard.is_empty() && !*shutdown.lock().unwrap() {
            tasks_guard = condvar.wait(tasks_guard).unwrap();
        }

        if *shutdown.lock().unwrap() && tasks_guard.is_empty() {
            return None;
        }

        // Find a task that can be executed (dependencies met)
        let mut available_task = None;
        let mut temp_tasks = Vec::new();

        while let Some(task) = tasks_guard.pop() {
            if Self::dependencies_satisfied(&task, completed_tasks) {
                available_task = Some(task);
                break;
            }
            // Check if any dependency failed
            if Self::has_failed_dependency(&task, failed_tasks) {
                // Mark task as failed due to dependency failure
                task_status
                    .lock()
                    .unwrap()
                    .insert(task.id, TaskStatus::Failed);
                results.lock().unwrap().push(TaskResult {
                    task_id: task.id,
                    status: TaskStatus::Failed,
                    error: Some("Dependency failed".to_string()),
                });
            } else {
                // Put back in queue
                temp_tasks.push(task);
            }
        }

        // Put back tasks that couldn't be executed yet
        for task in temp_tasks {
            tasks_guard.push(task);
        }

        available_task
    }

    fn execute_task(
        worker_id: usize,
        task: Task,
        task_status: &Arc<Mutex<HashMap<u64, TaskStatus>>>,
        completed_tasks: &Arc<Mutex<HashSet<u64>>>,
        failed_tasks: &Arc<Mutex<HashSet<u64>>>,
        results: &Arc<Mutex<Vec<TaskResult>>>,
        condvar: &Arc<Condvar>,
    ) {
        println!("Worker {worker_id} executing task: {}", task.name);

        // Mark task as running
        task_status
            .lock()
            .unwrap()
            .insert(task.id, TaskStatus::Running);

        // Execute task with panic handling
        let function = Arc::clone(&task.function);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| function()));

        match result {
            Ok(Ok(())) => {
                // Task completed successfully
                task_status
                    .lock()
                    .unwrap()
                    .insert(task.id, TaskStatus::Completed);
                completed_tasks.lock().unwrap().insert(task.id);
                results.lock().unwrap().push(TaskResult {
                    task_id: task.id,
                    status: TaskStatus::Completed,
                    error: None,
                });
                println!("Worker {worker_id} completed task: {}", task.name);
            }
            Ok(Err(e)) => {
                // Task failed with error
                task_status
                    .lock()
                    .unwrap()
                    .insert(task.id, TaskStatus::Failed);
                failed_tasks.lock().unwrap().insert(task.id);
                results.lock().unwrap().push(TaskResult {
                    task_id: task.id,
                    status: TaskStatus::Failed,
                    error: Some(e.to_string()),
                });
                println!("Worker {worker_id} failed task: {} - Error: {e}", task.name);
            }
            Err(_) => {
                // Task panicked
                task_status
                    .lock()
                    .unwrap()
                    .insert(task.id, TaskStatus::Failed);
                failed_tasks.lock().unwrap().insert(task.id);
                results.lock().unwrap().push(TaskResult {
                    task_id: task.id,
                    status: TaskStatus::Failed,
                    error: Some("Task panicked".to_string()),
                });
                println!("Worker {worker_id} - task panicked: {}", task.name);
            }
        }

        // Notify other workers that a task completed
        condvar.notify_all();
    }

    /// # Panics
    /// Panics if any mutex is poisoned or if condvar operations fail.
    #[must_use]
    pub fn start(&self) -> Vec<thread::JoinHandle<()>> {
        let mut handles = Vec::new();

        for worker_id in 0..self.worker_count {
            let tasks = Arc::clone(&self.tasks);
            let task_status = Arc::clone(&self.task_status);
            let completed_tasks = Arc::clone(&self.completed_tasks);
            let failed_tasks = Arc::clone(&self.failed_tasks);
            let results = Arc::clone(&self.results);
            let shutdown = Arc::clone(&self.shutdown);
            let condvar = Arc::clone(&self.condvar);

            let handle = thread::spawn(move || {
                loop {
                    let task = {
                        let mut tasks_guard = tasks.lock().unwrap();

                        // Wait for tasks or shutdown signal
                        while tasks_guard.is_empty() && !*shutdown.lock().unwrap() {
                            tasks_guard = condvar.wait(tasks_guard).unwrap();
                        }

                        if *shutdown.lock().unwrap() && tasks_guard.is_empty() {
                            break;
                        }

                        // Find a task that can be executed (dependencies met)
                        let mut available_task = None;
                        let mut temp_tasks = Vec::new();

                        while let Some(task) = tasks_guard.pop() {
                            if Self::dependencies_satisfied(&task, &completed_tasks) {
                                available_task = Some(task);
                                break;
                            }
                            // Check if any dependency failed
                            if Self::has_failed_dependency(&task, &failed_tasks) {
                                // Mark task as failed due to dependency failure
                                task_status
                                    .lock()
                                    .unwrap()
                                    .insert(task.id, TaskStatus::Failed);
                                results.lock().unwrap().push(TaskResult {
                                    task_id: task.id,
                                    status: TaskStatus::Failed,
                                    error: Some("Dependency failed".to_string()),
                                });
                            } else {
                                // Put back in queue
                                temp_tasks.push(task);
                            }
                        }

                        // Put back tasks that couldn't be executed yet
                        for task in temp_tasks {
                            tasks_guard.push(task);
                        }

                        available_task
                    };

                    if let Some(task) = task {
                        println!("Worker {worker_id} executing task: {}", task.name);

                        // Mark task as running
                        task_status
                            .lock()
                            .unwrap()
                            .insert(task.id, TaskStatus::Running);

                        // Execute task with panic handling
                        let function = Arc::clone(&task.function);
                        let result =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| function()));

                        match result {
                            Ok(Ok(())) => {
                                // Task completed successfully
                                task_status
                                    .lock()
                                    .unwrap()
                                    .insert(task.id, TaskStatus::Completed);
                                completed_tasks.lock().unwrap().insert(task.id);
                                results.lock().unwrap().push(TaskResult {
                                    task_id: task.id,
                                    status: TaskStatus::Completed,
                                    error: None,
                                });
                                println!("Worker {worker_id} completed task: {}", task.name);
                            }
                            Ok(Err(e)) => {
                                // Task failed with error
                                task_status
                                    .lock()
                                    .unwrap()
                                    .insert(task.id, TaskStatus::Failed);
                                failed_tasks.lock().unwrap().insert(task.id);
                                results.lock().unwrap().push(TaskResult {
                                    task_id: task.id,
                                    status: TaskStatus::Failed,
                                    error: Some(e.to_string()),
                                });
                                println!(
                                    "Worker {worker_id} failed task: {} - Error: {e}",
                                    task.name
                                );
                            }
                            Err(_) => {
                                // Task panicked
                                task_status
                                    .lock()
                                    .unwrap()
                                    .insert(task.id, TaskStatus::Failed);
                                failed_tasks.lock().unwrap().insert(task.id);
                                results.lock().unwrap().push(TaskResult {
                                    task_id: task.id,
                                    status: TaskStatus::Failed,
                                    error: Some("Task panicked".to_string()),
                                });
                                println!("Worker {worker_id} - task panicked: {}", task.name);
                            }
                        }

                        // Notify other workers that a task completed
                        condvar.notify_all();
                    } else {
                        // No available tasks, wait a bit
                        thread::sleep(Duration::from_millis(10));
                    }
                }

                println!("Worker {worker_id} shutting down");
            });

            handles.push(handle);
        }

        handles
    }

    /// # Panics
    /// Panics if the mutex is poisoned.
    pub fn shutdown(&self) {
        *self.shutdown.lock().unwrap() = true;
        self.condvar.notify_all();
    }

    /// # Panics
    /// Panics if the mutex is poisoned.
    #[must_use]
    pub fn get_task_status(&self, task_id: u64) -> Option<TaskStatus> {
        self.task_status.lock().unwrap().get(&task_id).cloned()
    }

    /// # Panics
    /// Panics if the mutex is poisoned.
    #[must_use]
    pub fn get_all_results(&self) -> Vec<TaskResult> {
        self.results.lock().unwrap().clone()
    }

    /// # Panics
    /// Panics if the mutex is poisoned.
    #[must_use]
    pub fn get_completion_stats(&self) -> (usize, usize, usize) {
        let results = self.results.lock().unwrap();
        let completed = results
            .iter()
            .filter(|r| r.status == TaskStatus::Completed)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == TaskStatus::Failed)
            .count();
        let total = results.len();
        drop(results);
        (completed, failed, total)
    }

    fn dependencies_satisfied(task: &Task, completed_tasks: &Arc<Mutex<HashSet<u64>>>) -> bool {
        let completed = completed_tasks.lock().unwrap();
        task.dependencies
            .iter()
            .all(|dep_id| completed.contains(dep_id))
    }

    fn has_failed_dependency(task: &Task, failed_tasks: &Arc<Mutex<HashSet<u64>>>) -> bool {
        let failed = failed_tasks.lock().unwrap();
        task.dependencies
            .iter()
            .any(|dep_id| failed.contains(dep_id))
    }
}

fn main() {
    let scheduler = Scheduler::new(2);

    let task_1 = Task::new(1, "Task 1".to_string(), 1, || {
        println!("Executing Task 1");
        Ok(())
    })
    .with_dependencies(vec![]);

    let task_2 = Task::new(2, "Task 2".to_string(), 2, || {
        println!("Executing Task 2");
        Ok(())
    })
    .with_dependencies(vec![]);

    scheduler.add_task(task_1);
    scheduler.add_task(task_2);
    let _handles = scheduler.start();

    // Wait for all tasks to complete
    scheduler.wait_for_completion();
}
