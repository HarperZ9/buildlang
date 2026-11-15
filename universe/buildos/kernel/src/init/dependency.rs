//! BuildOS Dependency Resolution
//!
//! Handles dependency resolution for unit startup ordering.

#![allow(dead_code)]

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use super::unit::Unit;

/// Dependency graph
pub struct DependencyGraph {
    /// Forward edges (unit -> depends on)
    depends_on: BTreeMap<String, BTreeSet<String>>,
    /// Reverse edges (unit -> depended by)
    depended_by: BTreeMap<String, BTreeSet<String>>,
    /// Hard dependencies (requires)
    requires: BTreeMap<String, BTreeSet<String>>,
    /// Ordering dependencies (after)
    after: BTreeMap<String, BTreeSet<String>>,
    /// Conflict relationships
    conflicts: BTreeMap<String, BTreeSet<String>>,
}

impl DependencyGraph {
    /// Create a new dependency graph
    pub fn new() -> Self {
        Self {
            depends_on: BTreeMap::new(),
            depended_by: BTreeMap::new(),
            requires: BTreeMap::new(),
            after: BTreeMap::new(),
            conflicts: BTreeMap::new(),
        }
    }

    /// Add a unit to the graph
    pub fn add_unit(&mut self, unit: &Unit) {
        let name = unit.name.clone();

        // Initialize sets
        self.depends_on.entry(name.clone()).or_insert_with(BTreeSet::new);
        self.depended_by.entry(name.clone()).or_insert_with(BTreeSet::new);
        self.requires.entry(name.clone()).or_insert_with(BTreeSet::new);
        self.after.entry(name.clone()).or_insert_with(BTreeSet::new);
        self.conflicts.entry(name.clone()).or_insert_with(BTreeSet::new);

        // Add requires dependencies
        for dep in &unit.config.requires {
            self.add_dependency(&name, dep, DependencyType::Requires);
        }

        // Add wants dependencies
        for dep in &unit.config.wants {
            self.add_dependency(&name, dep, DependencyType::Wants);
        }

        // Add after ordering
        for dep in &unit.config.after {
            self.add_ordering(&name, dep);
        }

        // Add before ordering (reverse)
        for dep in &unit.config.before {
            self.add_ordering(dep, &name);
        }

        // Add conflicts
        for conflict in &unit.config.conflicts {
            self.add_conflict(&name, conflict);
        }
    }

    /// Add a dependency edge
    fn add_dependency(&mut self, from: &str, to: &str, dep_type: DependencyType) {
        self.depends_on
            .entry(from.to_string())
            .or_insert_with(BTreeSet::new)
            .insert(to.to_string());

        self.depended_by
            .entry(to.to_string())
            .or_insert_with(BTreeSet::new)
            .insert(from.to_string());

        if matches!(dep_type, DependencyType::Requires) {
            self.requires
                .entry(from.to_string())
                .or_insert_with(BTreeSet::new)
                .insert(to.to_string());
        }
    }

    /// Add an ordering edge
    fn add_ordering(&mut self, from: &str, to: &str) {
        self.after
            .entry(from.to_string())
            .or_insert_with(BTreeSet::new)
            .insert(to.to_string());
    }

    /// Add a conflict
    fn add_conflict(&mut self, a: &str, b: &str) {
        self.conflicts
            .entry(a.to_string())
            .or_insert_with(BTreeSet::new)
            .insert(b.to_string());
        self.conflicts
            .entry(b.to_string())
            .or_insert_with(BTreeSet::new)
            .insert(a.to_string());
    }

    /// Get all dependencies of a unit
    pub fn dependencies(&self, name: &str) -> Vec<String> {
        self.depends_on
            .get(name)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get hard dependencies (requires)
    pub fn required_by(&self, name: &str) -> Vec<String> {
        self.requires
            .get(name)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get units that depend on this one
    pub fn dependents(&self, name: &str) -> Vec<String> {
        self.depended_by
            .get(name)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get units that must start after this one
    pub fn start_after(&self, name: &str) -> Vec<String> {
        self.after
            .get(name)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get conflicting units
    pub fn conflicts_with(&self, name: &str) -> Vec<String> {
        self.conflicts
            .get(name)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Check for dependency cycles
    pub fn find_cycle(&self) -> Option<Vec<String>> {
        let mut visited = BTreeSet::new();
        let mut rec_stack = BTreeSet::new();
        let mut path = Vec::new();

        for name in self.depends_on.keys() {
            if self.find_cycle_dfs(name, &mut visited, &mut rec_stack, &mut path) {
                return Some(path);
            }
        }

        None
    }

    /// DFS for cycle detection
    fn find_cycle_dfs(
        &self,
        name: &str,
        visited: &mut BTreeSet<String>,
        rec_stack: &mut BTreeSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        if rec_stack.contains(name) {
            path.push(name.to_string());
            return true;
        }

        if visited.contains(name) {
            return false;
        }

        visited.insert(name.to_string());
        rec_stack.insert(name.to_string());
        path.push(name.to_string());

        if let Some(deps) = self.depends_on.get(name) {
            for dep in deps {
                if self.find_cycle_dfs(dep, visited, rec_stack, path) {
                    return true;
                }
            }
        }

        path.pop();
        rec_stack.remove(name);
        false
    }

    /// Get a valid startup order using topological sort
    pub fn startup_order(&self, target: &str) -> Result<Vec<String>, DependencyError> {
        let mut result = Vec::new();
        let mut in_degree: BTreeMap<String, usize> = BTreeMap::new();
        let mut queue = VecDeque::new();

        // Get all units needed for target
        let needed = self.transitive_dependencies(target);

        // Calculate in-degrees
        for name in &needed {
            let degree = self.after
                .get(name)
                .map(|deps| deps.iter().filter(|d| needed.contains(*d)).count())
                .unwrap_or(0);
            in_degree.insert(name.clone(), degree);

            if degree == 0 {
                queue.push_back(name.clone());
            }
        }

        // Kahn's algorithm
        while let Some(name) = queue.pop_front() {
            result.push(name.clone());

            // Find units that depend on this one
            for dependent in &needed {
                if let Some(deps) = self.after.get(dependent) {
                    if deps.contains(&name) {
                        if let Some(degree) = in_degree.get_mut(dependent) {
                            *degree -= 1;
                            if *degree == 0 {
                                queue.push_back(dependent.clone());
                            }
                        }
                    }
                }
            }
        }

        if result.len() != needed.len() {
            return Err(DependencyError::CyclicDependency(
                self.find_cycle().unwrap_or_default()
            ));
        }

        Ok(result)
    }

    /// Get all transitive dependencies
    pub fn transitive_dependencies(&self, name: &str) -> BTreeSet<String> {
        let mut result = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(name.to_string());

        while let Some(current) = queue.pop_front() {
            if result.contains(&current) {
                continue;
            }
            result.insert(current.clone());

            if let Some(deps) = self.depends_on.get(&current) {
                for dep in deps {
                    if !result.contains(dep) {
                        queue.push_back(dep.clone());
                    }
                }
            }
        }

        result
    }

    /// Get shutdown order (reverse of startup)
    pub fn shutdown_order(&self, target: &str) -> Result<Vec<String>, DependencyError> {
        let mut order = self.startup_order(target)?;
        order.reverse();
        Ok(order)
    }

    /// Find units that can be started in parallel
    pub fn parallel_groups(&self, target: &str) -> Result<Vec<Vec<String>>, DependencyError> {
        let order = self.startup_order(target)?;
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut started: BTreeSet<String> = BTreeSet::new();

        for name in order {
            // Find which group this can go in
            let can_start_after: BTreeSet<String> = self.after
                .get(&name)
                .map(|deps| deps.iter().cloned().collect())
                .unwrap_or_default();

            // Find the group where all dependencies are satisfied
            let mut group_idx = 0;
            for (idx, group) in groups.iter().enumerate() {
                if group.iter().any(|g| can_start_after.contains(g)) {
                    group_idx = idx + 1;
                }
            }

            // Extend groups if needed
            while groups.len() <= group_idx {
                groups.push(Vec::new());
            }

            groups[group_idx].push(name.clone());
            started.insert(name);
        }

        Ok(groups)
    }
}

/// Dependency type
#[derive(Clone, Copy, Debug)]
pub enum DependencyType {
    /// Hard dependency (requires)
    Requires,
    /// Soft dependency (wants)
    Wants,
}

/// Dependency resolution errors
#[derive(Clone, Debug)]
pub enum DependencyError {
    /// Cyclic dependency detected
    CyclicDependency(Vec<String>),
    /// Missing dependency
    MissingDependency(String),
    /// Conflicting units
    Conflict(String, String),
}

/// Job transaction for atomic unit operations
pub struct Transaction {
    /// Jobs to execute
    jobs: Vec<Job>,
    /// Units to stop (conflicts)
    stops: Vec<String>,
}

/// A job in the transaction
#[derive(Clone, Debug)]
pub struct Job {
    /// Unit name
    pub unit: String,
    /// Job type
    pub job_type: JobType,
    /// Is this job required for transaction success
    pub required: bool,
}

/// Job type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobType {
    /// Start the unit
    Start,
    /// Stop the unit
    Stop,
    /// Reload the unit
    Reload,
    /// Restart the unit
    Restart,
    /// Verify dependencies (dry run)
    Verify,
    /// No operation
    Nop,
}

impl Transaction {
    /// Create a new transaction
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            stops: Vec::new(),
        }
    }

    /// Add a start job
    pub fn add_start(&mut self, unit: &str, required: bool) {
        self.jobs.push(Job {
            unit: unit.to_string(),
            job_type: JobType::Start,
            required,
        });
    }

    /// Add a stop job
    pub fn add_stop(&mut self, unit: &str) {
        self.jobs.push(Job {
            unit: unit.to_string(),
            job_type: JobType::Stop,
            required: false,
        });
    }

    /// Get jobs in execution order
    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    /// Check if transaction is empty
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

/// Build a transaction to start a target
pub fn build_start_transaction(
    graph: &DependencyGraph,
    target: &str,
    active: &BTreeSet<String>,
) -> Result<Transaction, DependencyError> {
    let mut transaction = Transaction::new();

    // Get startup order
    let order = graph.startup_order(target)?;

    // Add start jobs for units not already active
    for name in order {
        if !active.contains(&name) {
            // Check if required (in requires chain) or wanted
            let required = graph.transitive_dependencies(target)
                .iter()
                .any(|dep| graph.required_by(dep).contains(&name));

            transaction.add_start(&name, required);

            // Stop conflicting units
            for conflict in graph.conflicts_with(&name) {
                if active.contains(&conflict) {
                    transaction.add_stop(&conflict);
                }
            }
        }
    }

    Ok(transaction)
}

/// Build a transaction to stop a target
pub fn build_stop_transaction(
    graph: &DependencyGraph,
    target: &str,
    active: &BTreeSet<String>,
) -> Result<Transaction, DependencyError> {
    let mut transaction = Transaction::new();

    // Get shutdown order
    let order = graph.shutdown_order(target)?;

    // Add stop jobs for active units
    for name in order {
        if active.contains(&name) {
            transaction.add_stop(&name);
        }
    }

    Ok(transaction)
}
