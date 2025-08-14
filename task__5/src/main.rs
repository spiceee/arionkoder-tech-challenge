#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

type TaskId = u32;
type TaskResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub struct Task {
    id: TaskId,
    name: String,
    priority: u8,
    dependencies: Vec<TaskId>,
    function: Box<dyn Fn() -> TaskResult + Send + Sync>,
}

impl Task {
    pub fn new<F>(id: TaskId, name: String, priority: u8, function: F) -> Self
    where
        F: Fn() -> TaskResult + Send + Sync + 'static,
    {
        Self {
            id,
            name,
            priority,
            dependencies: Vec::new(),
            function: Box::new(function),
        }
    }

    #[must_use]
    pub fn with_dependencies(mut self, dependencies: Vec<TaskId>) -> Self {
        self.dependencies = dependencies;
        self
    }

    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }

    #[must_use]
    pub const fn priority(&self) -> u8 {
        self.priority
    }

    #[must_use]
    pub fn dependencies(&self) -> &[TaskId] {
        &self.dependencies
    }
}

#[derive(Clone)]
struct SharedState {
    task_queue: Arc<Mutex<VecDeque<Task>>>,
    task_status: Arc<Mutex<HashMap<TaskId, TaskStatus>>>,
    completed_tasks: Arc<Mutex<Vec<TaskId>>>,
    condvar: Arc<Condvar>,
    shutdown: Arc<Mutex<bool>>,
}

pub struct Scheduler {
    shared_state: SharedState,
    thread_count: usize,
}

impl Scheduler {
    #[must_use]
    pub fn new(thread_count: usize) -> Self {
        let shared_state = SharedState {
            task_queue: Arc::new(Mutex::new(VecDeque::new())),
            task_status: Arc::new(Mutex::new(HashMap::new())),
            completed_tasks: Arc::new(Mutex::new(Vec::new())),
            condvar: Arc::new(Condvar::new()),
            shutdown: Arc::new(Mutex::new(false)),
        };

        Self {
            shared_state,
            thread_count,
        }
    }

    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn add_task(&self, task: Task) {
        let task_id = task.id();

        {
            let mut status = self.shared_state.task_status.lock().unwrap();
            status.insert(task_id, TaskStatus::Pending);
        }

        {
            let mut queue = self.shared_state.task_queue.lock().unwrap();
            // Insert task maintaining priority order (higher priority first)
            let insert_pos = queue
                .iter()
                .position(|t| t.priority() < task.priority())
                .unwrap_or(queue.len());
            queue.insert(insert_pos, task);
        }

        self.shared_state.condvar.notify_one();
    }

    #[must_use]
    pub fn start(self) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        for thread_id in 0..self.thread_count {
            let state = self.shared_state.clone();

            let handle = thread::spawn(move || {
                Self::worker_thread(thread_id, &state);
            });

            handles.push(handle);
        }

        handles
    }

    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn shutdown(&self) {
        {
            let mut shutdown = self.shared_state.shutdown.lock().unwrap();
            *shutdown = true;
        }
        self.shared_state.condvar.notify_all();
    }

    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    #[must_use]
    pub fn get_task_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        let status = self.shared_state.task_status.lock().unwrap();
        status.get(&task_id).cloned()
    }

    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    #[must_use]
    pub fn get_all_statuses(&self) -> HashMap<TaskId, TaskStatus> {
        let status = self.shared_state.task_status.lock().unwrap();
        status.clone()
    }

    fn worker_thread(thread_id: usize, state: &SharedState) {
        loop {
            let task = {
                let mut queue = state.task_queue.lock().unwrap();

                loop {
                    let shutdown = *state.shutdown.lock().unwrap();
                    if shutdown {
                        return;
                    }

                    if let Some(task) = Self::find_ready_task(&mut queue, state) {
                        break Some(task);
                    }

                    queue = state
                        .condvar
                        .wait_timeout(queue, Duration::from_millis(100))
                        .unwrap()
                        .0;
                }
            };

            if let Some(task) = task {
                println!(
                    "Thread {} executing task: {} (ID: {})",
                    thread_id,
                    task.name,
                    task.id()
                );

                // Mark task as running
                {
                    let mut status = state.task_status.lock().unwrap();
                    status.insert(task.id(), TaskStatus::Running);
                }

                // Execute task with panic handling
                let task_id = task.id();
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (task.function)()));

                // Update task status based on result
                match result {
                    Ok(Ok(())) => {
                        {
                            let mut status = state.task_status.lock().unwrap();
                            status.insert(task_id, TaskStatus::Completed);
                        }
                        state.completed_tasks.lock().unwrap().push(task_id);
                        println!("Thread {thread_id} completed task ID: {task_id}");
                    }
                    Ok(Err(e)) => {
                        let mut status = state.task_status.lock().unwrap();
                        status.insert(task_id, TaskStatus::Failed);
                        drop(status);
                        eprintln!("Thread {thread_id} - Task {task_id} failed with error: {e}");
                    }
                    Err(_) => {
                        let mut status = state.task_status.lock().unwrap();
                        status.insert(task_id, TaskStatus::Failed);
                        drop(status);
                        eprintln!("Thread {thread_id} - Task {task_id} panicked");
                    }
                }

                // Notify other threads that a task completed (might unblock dependencies)
                state.condvar.notify_all();
            }
        }
    }

    fn find_ready_task(queue: &mut VecDeque<Task>, state: &SharedState) -> Option<Task> {
        let completed_tasks = state.completed_tasks.lock().unwrap();

        for i in 0..queue.len() {
            let task = &queue[i];

            // Check if all dependencies are completed
            let dependencies_met = task
                .dependencies()
                .iter()
                .all(|dep_id| completed_tasks.contains(dep_id));

            if dependencies_met {
                return queue.remove(i);
            }
        }

        None
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
}
