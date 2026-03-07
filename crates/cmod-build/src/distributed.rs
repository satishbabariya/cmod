//! Distributed build execution and remote worker management.
//!
//! Implements the worker-based distribution model from RFC-0013:
//! - Worker registration and health monitoring
//! - Task scheduling with work stealing
//! - HTTP-based worker protocol for remote compilation
//! - Result collection and artifact assembly

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;

use crate::plan::BuildNode;

/// Describes a remote build worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    /// Unique worker identifier.
    pub id: String,
    /// Worker endpoint URL (e.g., `http://worker1:9090`).
    pub endpoint: String,
    /// Target triple this worker supports.
    pub target: String,
    /// Compiler available on this worker.
    pub compiler: String,
    /// Compiler version string.
    pub compiler_version: String,
    /// Maximum concurrent compilation jobs.
    pub max_jobs: usize,
    /// Current number of running jobs.
    pub active_jobs: usize,
    /// Worker health status.
    pub status: WorkerStatus,
    /// Last heartbeat timestamp (seconds since epoch).
    pub last_heartbeat: u64,
}

/// Worker health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerStatus {
    /// Worker is ready to accept jobs.
    Ready,
    /// Worker is busy but can queue more work.
    Busy,
    /// Worker is at capacity.
    Full,
    /// Worker is unreachable or unhealthy.
    Offline,
    /// Worker is shutting down (draining).
    Draining,
}

/// A compilation task sent to a remote worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTask {
    /// Unique task identifier.
    pub task_id: String,
    /// Build node to compile.
    pub node_id: String,
    /// Source file path (relative to project root).
    pub source_file: String,
    /// Compiler command to execute.
    pub command: Vec<String>,
    /// Working directory on the worker.
    pub working_dir: String,
    /// Required input files (BMIs, headers) that must be transferred.
    pub inputs: Vec<RemoteFileRef>,
    /// Expected output files.
    pub expected_outputs: Vec<String>,
}

/// Reference to a file for transfer to/from a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFileRef {
    /// Logical path (relative).
    pub path: String,
    /// SHA-256 content hash for integrity verification.
    pub hash: String,
    /// File size in bytes.
    pub size: u64,
}

/// Result of a remote compilation task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTaskResult {
    /// Task identifier.
    pub task_id: String,
    /// Whether compilation succeeded.
    pub success: bool,
    /// Compiler stdout.
    pub stdout: String,
    /// Compiler stderr.
    pub stderr: String,
    /// Exit code.
    pub exit_code: i32,
    /// Output artifacts produced.
    pub outputs: Vec<RemoteFileRef>,
    /// Compilation duration in milliseconds.
    pub duration_ms: u64,
}

/// Configuration for distributed builds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedConfig {
    /// Whether distributed builds are enabled.
    pub enabled: bool,
    /// Worker endpoints.
    pub workers: Vec<String>,
    /// Scheduler strategy.
    pub scheduler: SchedulerStrategy,
    /// Maximum tasks to queue per worker.
    pub max_queue_depth: usize,
    /// Timeout for worker health checks (seconds).
    pub health_check_timeout: u64,
    /// Timeout for individual compilation tasks (seconds).
    pub task_timeout: u64,
    /// Authentication token for worker communication.
    pub auth_token: Option<String>,
}

impl Default for DistributedConfig {
    fn default() -> Self {
        DistributedConfig {
            enabled: false,
            workers: Vec::new(),
            scheduler: SchedulerStrategy::LeastLoaded,
            max_queue_depth: 16,
            health_check_timeout: 5,
            task_timeout: 300,
            auth_token: None,
        }
    }
}

/// Strategy for distributing tasks to workers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerStrategy {
    /// Send to the worker with the fewest active jobs.
    LeastLoaded,
    /// Round-robin assignment.
    RoundRobin,
    /// Assign based on target triple affinity.
    TargetAffinity,
}

/// Manages a pool of remote workers and task distribution.
pub struct WorkerPool {
    workers: Arc<Mutex<Vec<WorkerInfo>>>,
    strategy: SchedulerStrategy,
    _task_queue: Arc<Mutex<VecDeque<RemoteTask>>>,
    results: Arc<Mutex<BTreeMap<String, RemoteTaskResult>>>,
    /// Maps task_id → worker_id so collect_result can release the correct slot.
    task_workers: Arc<Mutex<BTreeMap<String, String>>>,
    auth_token: Option<String>,
    task_timeout: Duration,
    round_robin_idx: Arc<Mutex<usize>>,
}

impl WorkerPool {
    /// Create a new worker pool from configuration.
    pub fn new(config: &DistributedConfig) -> Self {
        let workers: Vec<WorkerInfo> = config
            .workers
            .iter()
            .enumerate()
            .map(|(i, endpoint)| WorkerInfo {
                id: format!("worker-{}", i),
                endpoint: endpoint.clone(),
                target: String::new(),
                compiler: String::new(),
                compiler_version: String::new(),
                max_jobs: 4,
                active_jobs: 0,
                status: WorkerStatus::Offline,
                last_heartbeat: 0,
            })
            .collect();

        WorkerPool {
            workers: Arc::new(Mutex::new(workers)),
            strategy: config.scheduler,
            _task_queue: Arc::new(Mutex::new(VecDeque::new())),
            results: Arc::new(Mutex::new(BTreeMap::new())),
            task_workers: Arc::new(Mutex::new(BTreeMap::new())),
            auth_token: config.auth_token.clone(),
            task_timeout: Duration::from_secs(config.task_timeout),
            round_robin_idx: Arc::new(Mutex::new(0)),
        }
    }

    /// Discover and register workers by health-checking each endpoint.
    pub fn discover_workers(&self) -> Result<usize, CmodError> {
        let mut workers = self
            .workers
            .lock()
            .map_err(|_| CmodError::Other("failed to lock worker pool".to_string()))?;

        let mut available = 0;

        for worker in workers.iter_mut() {
            match self.health_check(&worker.endpoint) {
                Ok(info) => {
                    worker.target = info.target;
                    worker.compiler = info.compiler;
                    worker.compiler_version = info.compiler_version;
                    worker.max_jobs = info.max_jobs;
                    worker.status = WorkerStatus::Ready;
                    worker.last_heartbeat = now_secs();
                    available += 1;
                }
                Err(_) => {
                    worker.status = WorkerStatus::Offline;
                }
            }
        }

        Ok(available)
    }

    /// Select the best worker for a task based on the scheduling strategy.
    pub fn select_worker(&self, _task: &RemoteTask) -> Option<String> {
        let workers = self.workers.lock().ok()?;

        let available: Vec<&WorkerInfo> = workers
            .iter()
            .filter(|w| w.status == WorkerStatus::Ready || w.status == WorkerStatus::Busy)
            .filter(|w| w.active_jobs < w.max_jobs)
            .collect();

        if available.is_empty() {
            return None;
        }

        match self.strategy {
            SchedulerStrategy::LeastLoaded => available
                .iter()
                .min_by_key(|w| w.active_jobs)
                .map(|w| w.id.clone()),
            SchedulerStrategy::RoundRobin => {
                let mut idx = self.round_robin_idx.lock().ok()?;
                let worker = &available[*idx % available.len()];
                *idx = (*idx + 1) % available.len();
                Some(worker.id.clone())
            }
            SchedulerStrategy::TargetAffinity => {
                // For now, just use least-loaded; target matching would compare
                // task.target against worker.target
                available
                    .iter()
                    .min_by_key(|w| w.active_jobs)
                    .map(|w| w.id.clone())
            }
        }
    }

    /// Submit a task to a worker.
    pub fn submit_task(&self, worker_id: &str, task: RemoteTask) -> Result<(), CmodError> {
        let task_id = task.task_id.clone();

        // Extract the endpoint while holding the workers lock briefly.
        let endpoint = {
            let workers = self
                .workers
                .lock()
                .map_err(|_| CmodError::Other("failed to lock worker pool".to_string()))?;
            let worker = workers
                .iter()
                .find(|w| w.id == worker_id)
                .ok_or_else(|| CmodError::Other(format!("worker '{}' not found", worker_id)))?;
            worker.endpoint.clone()
        };

        // Perform the (potentially slow) HTTP call without holding any lock.
        let result = self.send_task(&endpoint, &task)?;

        // Update the worker's bookkeeping.
        {
            let mut workers = self
                .workers
                .lock()
                .map_err(|_| CmodError::Other("failed to lock worker pool".to_string()))?;
            if let Some(worker) = workers.iter_mut().find(|w| w.id == worker_id) {
                worker.active_jobs += 1;
                if worker.active_jobs >= worker.max_jobs {
                    worker.status = WorkerStatus::Full;
                } else {
                    worker.status = WorkerStatus::Busy;
                }
            }
        }

        // Store the result so collect_result() can return it.
        {
            let mut results = self
                .results
                .lock()
                .map_err(|_| CmodError::Other("failed to lock results".to_string()))?;
            results.insert(task_id.clone(), result);
        }

        // Record which worker owns this task for slot release in collect_result().
        {
            let mut tw = self
                .task_workers
                .lock()
                .map_err(|_| CmodError::Other("failed to lock task workers".to_string()))?;
            tw.insert(task_id, worker_id.to_string());
        }

        Ok(())
    }

    /// Collect a completed task result from a worker.
    ///
    /// Removes the result from the map **and** releases the originating
    /// worker's slot (decrements `active_jobs`, transitions status away
    /// from `Full` when appropriate).
    pub fn collect_result(&self, task_id: &str) -> Option<RemoteTaskResult> {
        let result = {
            let mut results = self.results.lock().ok()?;
            results.remove(task_id)?
        };

        // Look up which worker owned this task and release its slot.
        let worker_id = {
            let mut tw = self.task_workers.lock().ok()?;
            tw.remove(task_id)
        };

        if let Some(worker_id) = worker_id {
            if let Ok(mut workers) = self.workers.lock() {
                if let Some(worker) = workers.iter_mut().find(|w| w.id == worker_id) {
                    worker.active_jobs = worker.active_jobs.saturating_sub(1);
                    if worker.active_jobs == 0 {
                        worker.status = WorkerStatus::Ready;
                    } else if worker.active_jobs < worker.max_jobs {
                        worker.status = WorkerStatus::Busy;
                    }
                }
            }
        }

        Some(result)
    }

    /// Get the list of all registered workers.
    pub fn list_workers(&self) -> Vec<WorkerInfo> {
        self.workers.lock().map(|w| w.clone()).unwrap_or_default()
    }

    /// Get count of available workers.
    pub fn available_count(&self) -> usize {
        self.workers
            .lock()
            .map(|workers| {
                workers
                    .iter()
                    .filter(|w| matches!(w.status, WorkerStatus::Ready | WorkerStatus::Busy))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Health-check a single worker endpoint.
    fn health_check(&self, endpoint: &str) -> Result<WorkerInfo, CmodError> {
        let url = format!("{}/health", endpoint.trim_end_matches('/'));

        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .http_status_as_error(false)
                .build(),
        );

        let mut req = agent.get(&url);
        if let Some(ref token) = self.auth_token {
            req = req.header("Authorization", &format!("Bearer {}", token));
        }

        let resp = req.call().map_err(|e| {
            CmodError::Other(format!("health check failed for {}: {}", endpoint, e))
        })?;

        if resp.status().as_u16() != 200 {
            return Err(CmodError::Other(format!(
                "worker {} returned HTTP {}",
                endpoint,
                resp.status()
            )));
        }

        let body: String = resp
            .into_body()
            .read_to_string()
            .map_err(|e| CmodError::Other(format!("failed to read health response: {}", e)))?;

        serde_json::from_str(&body)
            .map_err(|e| CmodError::Other(format!("invalid health response: {}", e)))
    }

    /// Send a compilation task to a worker and return the result.
    fn send_task(&self, endpoint: &str, task: &RemoteTask) -> Result<RemoteTaskResult, CmodError> {
        let url = format!("{}/tasks", endpoint.trim_end_matches('/'));

        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(self.task_timeout))
                .http_status_as_error(false)
                .build(),
        );

        let body = serde_json::to_string(task)
            .map_err(|e| CmodError::Other(format!("failed to serialize task: {}", e)))?;

        let mut req = agent.post(&url);
        if let Some(ref token) = self.auth_token {
            req = req.header("Authorization", &format!("Bearer {}", token));
        }
        req = req.header("Content-Type", "application/json");

        let resp = req
            .send(&body)
            .map_err(|e| CmodError::Other(format!("failed to submit task: {}", e)))?;

        if !(200..300).contains(&resp.status().as_u16()) {
            return Err(CmodError::Other(format!(
                "worker rejected task with HTTP {}",
                resp.status()
            )));
        }

        let resp_body: String = resp
            .into_body()
            .read_to_string()
            .map_err(|e| CmodError::Other(format!("failed to read task response: {}", e)))?;

        serde_json::from_str(&resp_body)
            .map_err(|e| CmodError::Other(format!("invalid task response: {}", e)))
    }
}

/// Convert build plan nodes to remote tasks.
pub fn nodes_to_remote_tasks(nodes: &[BuildNode], project_root: &Path) -> Vec<RemoteTask> {
    nodes
        .iter()
        .filter(|n| {
            matches!(
                n.kind,
                cmod_core::types::NodeKind::Interface
                    | cmod_core::types::NodeKind::Implementation
                    | cmod_core::types::NodeKind::Object
            )
        })
        .map(|node| {
            let source_file = node
                .source
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            RemoteTask {
                task_id: format!("task-{}", &node.id),
                node_id: node.id.clone(),
                source_file,
                command: Vec::new(), // Command is generated by the worker's compiler backend
                working_dir: project_root.display().to_string(),
                inputs: Vec::new(),
                expected_outputs: node
                    .outputs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect(),
            }
        })
        .collect()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distributed_config_default() {
        let config = DistributedConfig::default();
        assert!(!config.enabled);
        assert!(config.workers.is_empty());
        assert_eq!(config.scheduler, SchedulerStrategy::LeastLoaded);
    }

    #[test]
    fn test_worker_pool_creation() {
        let config = DistributedConfig {
            workers: vec!["http://worker1:9090".into(), "http://worker2:9090".into()],
            ..Default::default()
        };
        let pool = WorkerPool::new(&config);
        let workers = pool.list_workers();
        assert_eq!(workers.len(), 2);
        assert_eq!(workers[0].status, WorkerStatus::Offline);
    }

    #[test]
    fn test_select_worker_empty_pool() {
        let config = DistributedConfig::default();
        let pool = WorkerPool::new(&config);
        let task = RemoteTask {
            task_id: "t1".into(),
            node_id: "n1".into(),
            source_file: "test.cpp".into(),
            command: vec![],
            working_dir: "/tmp".into(),
            inputs: vec![],
            expected_outputs: vec![],
        };
        assert!(pool.select_worker(&task).is_none());
    }

    #[test]
    fn test_select_worker_least_loaded() {
        let config = DistributedConfig {
            workers: vec!["http://w1:9090".into(), "http://w2:9090".into()],
            scheduler: SchedulerStrategy::LeastLoaded,
            ..Default::default()
        };
        let pool = WorkerPool::new(&config);

        // Set one worker as ready with 0 jobs, another with 2 jobs
        {
            let mut workers = pool.workers.lock().unwrap();
            workers[0].status = WorkerStatus::Ready;
            workers[0].active_jobs = 2;
            workers[1].status = WorkerStatus::Ready;
            workers[1].active_jobs = 0;
        }

        let task = RemoteTask {
            task_id: "t1".into(),
            node_id: "n1".into(),
            source_file: "test.cpp".into(),
            command: vec![],
            working_dir: "/tmp".into(),
            inputs: vec![],
            expected_outputs: vec![],
        };

        let selected = pool.select_worker(&task).unwrap();
        assert_eq!(selected, "worker-1"); // worker with 0 jobs
    }

    #[test]
    fn test_available_count() {
        let config = DistributedConfig {
            workers: vec!["http://w1:9090".into(), "http://w2:9090".into()],
            ..Default::default()
        };
        let pool = WorkerPool::new(&config);
        assert_eq!(pool.available_count(), 0);

        {
            let mut workers = pool.workers.lock().unwrap();
            workers[0].status = WorkerStatus::Ready;
        }

        assert_eq!(pool.available_count(), 1);
    }

    #[test]
    fn test_worker_status_serde() {
        let json = serde_json::to_string(&WorkerStatus::Ready).unwrap();
        assert_eq!(json, "\"ready\"");
        let parsed: WorkerStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, WorkerStatus::Ready);
    }

    #[test]
    fn test_scheduler_strategy_serde() {
        let json = serde_json::to_string(&SchedulerStrategy::RoundRobin).unwrap();
        assert_eq!(json, "\"round_robin\"");
    }

    #[test]
    fn test_remote_task_serialization() {
        let task = RemoteTask {
            task_id: "t-001".into(),
            node_id: "interface_fmt".into(),
            source_file: "src/fmt.cppm".into(),
            command: vec!["clang++".into(), "-std=c++20".into()],
            working_dir: "/project".into(),
            inputs: vec![RemoteFileRef {
                path: "build/deps/base.pcm".into(),
                hash: "abc123".into(),
                size: 1024,
            }],
            expected_outputs: vec!["build/fmt.pcm".into()],
        };
        let json = serde_json::to_string(&task).unwrap();
        let parsed: RemoteTask = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task_id, "t-001");
        assert_eq!(parsed.inputs.len(), 1);
    }
}
