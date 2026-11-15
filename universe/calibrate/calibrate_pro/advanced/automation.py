"""
Automation API Module

Provides scripting and automation capabilities:
- Python scripting interface
- Batch calibration workflows
- CI/CD integration for studios
- Command-line interface
- Task scheduling
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Optional, Callable, Any, Dict, List, Union
from datetime import datetime, timedelta
from pathlib import Path
import json
import asyncio
import logging
import threading
import queue
import sys
from concurrent.futures import ThreadPoolExecutor, Future

# Setup logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("calibrate_pro.automation")

# =============================================================================
# Enums
# =============================================================================

class TaskStatus(Enum):
    """Automation task status."""
    PENDING = auto()
    RUNNING = auto()
    COMPLETED = auto()
    FAILED = auto()
    CANCELLED = auto()
    SKIPPED = auto()


class TaskType(Enum):
    """Type of automation task."""
    CALIBRATE = auto()
    VERIFY = auto()
    PROFILE = auto()
    LUT_GENERATE = auto()
    LUT_APPLY = auto()
    MEASURE = auto()
    EXPORT = auto()
    CUSTOM = auto()


class WorkflowState(Enum):
    """Workflow execution state."""
    IDLE = auto()
    RUNNING = auto()
    PAUSED = auto()
    COMPLETED = auto()
    FAILED = auto()


class EventType(Enum):
    """Automation event types."""
    TASK_STARTED = auto()
    TASK_COMPLETED = auto()
    TASK_FAILED = auto()
    WORKFLOW_STARTED = auto()
    WORKFLOW_COMPLETED = auto()
    WORKFLOW_FAILED = auto()
    MEASUREMENT_TAKEN = auto()
    PROFILE_GENERATED = auto()
    LUT_APPLIED = auto()


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class TaskResult:
    """Result of a task execution."""
    success: bool
    data: Dict[str, Any] = field(default_factory=dict)
    error: Optional[str] = None
    duration: float = 0.0
    timestamp: datetime = field(default_factory=datetime.now)


@dataclass
class AutomationTask:
    """Single automation task definition."""
    task_id: str
    task_type: TaskType
    name: str
    description: str = ""

    # Task parameters
    parameters: Dict[str, Any] = field(default_factory=dict)

    # Dependencies
    depends_on: List[str] = field(default_factory=list)

    # Execution
    status: TaskStatus = TaskStatus.PENDING
    result: Optional[TaskResult] = None
    retry_count: int = 0
    max_retries: int = 3

    # Timing
    timeout: float = 300.0  # 5 minutes default
    created_at: datetime = field(default_factory=datetime.now)
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None


@dataclass
class Workflow:
    """Workflow definition containing multiple tasks."""
    workflow_id: str
    name: str
    description: str = ""

    # Tasks in execution order
    tasks: List[AutomationTask] = field(default_factory=list)

    # State
    state: WorkflowState = WorkflowState.IDLE
    current_task_index: int = 0
    progress: float = 0.0

    # Results
    results: Dict[str, TaskResult] = field(default_factory=dict)
    errors: List[str] = field(default_factory=list)

    # Timing
    created_at: datetime = field(default_factory=datetime.now)
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None

    # Metadata
    created_by: str = ""
    tags: List[str] = field(default_factory=list)


@dataclass
class AutomationEvent:
    """Automation system event."""
    event_type: EventType
    timestamp: datetime = field(default_factory=datetime.now)
    data: Dict[str, Any] = field(default_factory=dict)
    source: str = ""


@dataclass
class ScheduledTask:
    """Scheduled task for recurring execution."""
    schedule_id: str
    workflow_id: str
    name: str

    # Schedule
    cron_expression: str = ""  # Cron-style schedule
    interval_seconds: Optional[int] = None
    next_run: Optional[datetime] = None
    last_run: Optional[datetime] = None

    # State
    enabled: bool = True
    run_count: int = 0


# =============================================================================
# Task Handlers
# =============================================================================

class TaskHandler:
    """Base class for task handlers."""

    def __init__(self):
        self.name = "base"

    async def execute(self,
                      task: AutomationTask,
                      context: Dict[str, Any]) -> TaskResult:
        """Execute the task. Override in subclasses."""
        raise NotImplementedError

    def validate(self, task: AutomationTask) -> bool:
        """Validate task parameters."""
        return True


class CalibrateHandler(TaskHandler):
    """Handler for calibration tasks."""

    def __init__(self):
        super().__init__()
        self.name = "calibrate"

    async def execute(self,
                      task: AutomationTask,
                      context: Dict[str, Any]) -> TaskResult:
        """Execute calibration."""
        params = task.parameters
        display_id = params.get("display_id", 0)
        target_whitepoint = params.get("whitepoint", "D65")
        target_gamma = params.get("gamma", 2.2)

        logger.info(f"Starting calibration for display {display_id}")
        logger.info(f"Target: {target_whitepoint}, gamma {target_gamma}")

        # Simulate calibration
        await asyncio.sleep(0.5)

        return TaskResult(
            success=True,
            data={
                "display_id": display_id,
                "delta_e_mean": 1.2,
                "delta_e_max": 2.8,
                "profile_path": f"calibration_{display_id}.icc",
            }
        )

    def validate(self, task: AutomationTask) -> bool:
        """Validate calibration parameters."""
        params = task.parameters
        if "display_id" not in params:
            return False
        return True


class VerifyHandler(TaskHandler):
    """Handler for verification tasks."""

    def __init__(self):
        super().__init__()
        self.name = "verify"

    async def execute(self,
                      task: AutomationTask,
                      context: Dict[str, Any]) -> TaskResult:
        """Execute verification."""
        params = task.parameters
        display_id = params.get("display_id", 0)
        verification_type = params.get("type", "colorchecker")

        logger.info(f"Verifying display {display_id} with {verification_type}")

        await asyncio.sleep(0.3)

        return TaskResult(
            success=True,
            data={
                "display_id": display_id,
                "verification_type": verification_type,
                "delta_e_mean": 1.5,
                "passed": True,
            }
        )


class ProfileHandler(TaskHandler):
    """Handler for profile generation tasks."""

    def __init__(self):
        super().__init__()
        self.name = "profile"

    async def execute(self,
                      task: AutomationTask,
                      context: Dict[str, Any]) -> TaskResult:
        """Generate ICC profile."""
        params = task.parameters
        output_path = params.get("output_path", "profile.icc")

        logger.info(f"Generating profile: {output_path}")

        await asyncio.sleep(0.2)

        return TaskResult(
            success=True,
            data={
                "profile_path": output_path,
                "profile_version": "4.4",
            }
        )


class LUTHandler(TaskHandler):
    """Handler for LUT generation/application tasks."""

    def __init__(self):
        super().__init__()
        self.name = "lut"

    async def execute(self,
                      task: AutomationTask,
                      context: Dict[str, Any]) -> TaskResult:
        """Generate or apply LUT."""
        params = task.parameters
        action = params.get("action", "generate")
        lut_path = params.get("lut_path", "calibration.cube")

        logger.info(f"LUT {action}: {lut_path}")

        await asyncio.sleep(0.2)

        return TaskResult(
            success=True,
            data={
                "action": action,
                "lut_path": lut_path,
                "lut_size": 33,
            }
        )


class ExportHandler(TaskHandler):
    """Handler for export tasks."""

    def __init__(self):
        super().__init__()
        self.name = "export"

    async def execute(self,
                      task: AutomationTask,
                      context: Dict[str, Any]) -> TaskResult:
        """Export data/reports."""
        params = task.parameters
        export_type = params.get("type", "report")
        output_path = params.get("output_path", "report.pdf")

        logger.info(f"Exporting {export_type}: {output_path}")

        await asyncio.sleep(0.1)

        return TaskResult(
            success=True,
            data={
                "export_type": export_type,
                "output_path": output_path,
            }
        )


class CustomHandler(TaskHandler):
    """Handler for custom Python script tasks."""

    def __init__(self):
        super().__init__()
        self.name = "custom"

    async def execute(self,
                      task: AutomationTask,
                      context: Dict[str, Any]) -> TaskResult:
        """Execute custom script."""
        params = task.parameters
        script = params.get("script", "")
        script_path = params.get("script_path")

        if script_path:
            with open(script_path, 'r') as f:
                script = f.read()

        if not script:
            return TaskResult(
                success=False,
                error="No script provided",
            )

        # Execute script in isolated context
        local_vars: Dict[str, Any] = {"context": context, "result": {}}

        try:
            exec(script, {"__builtins__": __builtins__}, local_vars)
            return TaskResult(
                success=True,
                data=local_vars.get("result", {}),
            )
        except Exception as e:
            return TaskResult(
                success=False,
                error=str(e),
            )


# =============================================================================
# Workflow Engine
# =============================================================================

class WorkflowEngine:
    """
    Executes automation workflows.

    Manages task execution, dependencies, and error handling.
    """

    def __init__(self):
        # Task handlers
        self.handlers: Dict[TaskType, TaskHandler] = {
            TaskType.CALIBRATE: CalibrateHandler(),
            TaskType.VERIFY: VerifyHandler(),
            TaskType.PROFILE: ProfileHandler(),
            TaskType.LUT_GENERATE: LUTHandler(),
            TaskType.LUT_APPLY: LUTHandler(),
            TaskType.EXPORT: ExportHandler(),
            TaskType.CUSTOM: CustomHandler(),
        }

        # Event listeners
        self._event_listeners: List[Callable[[AutomationEvent], None]] = []

        # Execution context
        self._context: Dict[str, Any] = {}
        self._executor = ThreadPoolExecutor(max_workers=4)

    def register_handler(self,
                        task_type: TaskType,
                        handler: TaskHandler) -> None:
        """Register a custom task handler."""
        self.handlers[task_type] = handler

    def add_event_listener(self,
                          listener: Callable[[AutomationEvent], None]) -> None:
        """Add event listener."""
        self._event_listeners.append(listener)

    def remove_event_listener(self,
                             listener: Callable[[AutomationEvent], None]) -> None:
        """Remove event listener."""
        if listener in self._event_listeners:
            self._event_listeners.remove(listener)

    def _emit_event(self, event: AutomationEvent) -> None:
        """Emit event to listeners."""
        for listener in self._event_listeners:
            try:
                listener(event)
            except Exception as e:
                logger.error(f"Event listener error: {e}")

    async def execute_workflow(self,
                               workflow: Workflow,
                               context: Optional[Dict[str, Any]] = None) -> Workflow:
        """
        Execute a complete workflow.

        Args:
            workflow: Workflow to execute
            context: Optional execution context

        Returns:
            Updated workflow with results
        """
        self._context = context or {}
        workflow.state = WorkflowState.RUNNING
        workflow.started_at = datetime.now()

        self._emit_event(AutomationEvent(
            event_type=EventType.WORKFLOW_STARTED,
            data={"workflow_id": workflow.workflow_id, "name": workflow.name},
        ))

        try:
            # Build dependency graph
            task_map = {t.task_id: t for t in workflow.tasks}
            completed_tasks: set = set()

            while len(completed_tasks) < len(workflow.tasks):
                # Find tasks ready to execute
                ready_tasks = []
                for task in workflow.tasks:
                    if task.task_id in completed_tasks:
                        continue
                    if task.status == TaskStatus.SKIPPED:
                        completed_tasks.add(task.task_id)
                        continue

                    # Check dependencies
                    deps_met = all(
                        dep_id in completed_tasks
                        for dep_id in task.depends_on
                    )
                    if deps_met:
                        ready_tasks.append(task)

                if not ready_tasks:
                    if len(completed_tasks) < len(workflow.tasks):
                        # Circular dependency or missing dependency
                        workflow.errors.append("Workflow stuck: circular or missing dependencies")
                        break
                    break

                # Execute ready tasks (could be parallelized)
                for task in ready_tasks:
                    result = await self._execute_task(task)
                    workflow.results[task.task_id] = result
                    completed_tasks.add(task.task_id)

                    if not result.success and task.status == TaskStatus.FAILED:
                        workflow.errors.append(f"Task {task.name} failed: {result.error}")

                # Update progress
                workflow.current_task_index = len(completed_tasks)
                workflow.progress = len(completed_tasks) / len(workflow.tasks)

            # Determine final state
            if workflow.errors:
                workflow.state = WorkflowState.FAILED
                self._emit_event(AutomationEvent(
                    event_type=EventType.WORKFLOW_FAILED,
                    data={"workflow_id": workflow.workflow_id, "errors": workflow.errors},
                ))
            else:
                workflow.state = WorkflowState.COMPLETED
                self._emit_event(AutomationEvent(
                    event_type=EventType.WORKFLOW_COMPLETED,
                    data={"workflow_id": workflow.workflow_id},
                ))

        except Exception as e:
            workflow.state = WorkflowState.FAILED
            workflow.errors.append(str(e))
            logger.error(f"Workflow execution error: {e}")

        workflow.completed_at = datetime.now()
        return workflow

    async def _execute_task(self, task: AutomationTask) -> TaskResult:
        """Execute a single task."""
        task.status = TaskStatus.RUNNING
        task.started_at = datetime.now()

        self._emit_event(AutomationEvent(
            event_type=EventType.TASK_STARTED,
            data={"task_id": task.task_id, "name": task.name},
        ))

        handler = self.handlers.get(task.task_type)
        if not handler:
            result = TaskResult(
                success=False,
                error=f"No handler for task type: {task.task_type}",
            )
            task.status = TaskStatus.FAILED
            task.result = result
            return result

        # Validate task
        if not handler.validate(task):
            result = TaskResult(
                success=False,
                error="Task validation failed",
            )
            task.status = TaskStatus.FAILED
            task.result = result
            return result

        # Execute with timeout and retries
        start_time = datetime.now()

        for attempt in range(task.max_retries + 1):
            try:
                result = await asyncio.wait_for(
                    handler.execute(task, self._context),
                    timeout=task.timeout
                )
                break
            except asyncio.TimeoutError:
                result = TaskResult(
                    success=False,
                    error=f"Task timed out after {task.timeout}s",
                )
            except Exception as e:
                result = TaskResult(
                    success=False,
                    error=str(e),
                )

            task.retry_count = attempt + 1
            if attempt < task.max_retries:
                logger.warning(f"Task {task.name} failed, retrying ({attempt + 1}/{task.max_retries})")
                await asyncio.sleep(1)

        # Update task
        result.duration = (datetime.now() - start_time).total_seconds()
        task.result = result
        task.completed_at = datetime.now()

        if result.success:
            task.status = TaskStatus.COMPLETED
            self._emit_event(AutomationEvent(
                event_type=EventType.TASK_COMPLETED,
                data={"task_id": task.task_id, "result": result.data},
            ))
        else:
            task.status = TaskStatus.FAILED
            self._emit_event(AutomationEvent(
                event_type=EventType.TASK_FAILED,
                data={"task_id": task.task_id, "error": result.error},
            ))

        # Store result in context for downstream tasks
        self._context[f"task_{task.task_id}"] = result.data

        return result

    async def execute_task(self,
                          task: AutomationTask,
                          context: Optional[Dict[str, Any]] = None) -> TaskResult:
        """Execute a single task standalone."""
        self._context = context or {}
        return await self._execute_task(task)


# =============================================================================
# Automation API
# =============================================================================

class AutomationAPI:
    """
    High-level automation API for scripting and CI/CD integration.

    Provides a simple interface for common automation tasks.
    """

    def __init__(self):
        self.engine = WorkflowEngine()
        self._workflows: Dict[str, Workflow] = {}
        self._scheduled_tasks: Dict[str, ScheduledTask] = {}
        self._task_counter = 0
        self._workflow_counter = 0

    # =========================================================================
    # Task Creation
    # =========================================================================

    def create_task(self,
                    task_type: TaskType,
                    name: str,
                    parameters: Optional[Dict[str, Any]] = None,
                    **kwargs) -> AutomationTask:
        """Create a new automation task."""
        self._task_counter += 1
        task_id = f"task_{self._task_counter:04d}"

        return AutomationTask(
            task_id=task_id,
            task_type=task_type,
            name=name,
            parameters=parameters or {},
            **kwargs,
        )

    def calibrate(self,
                  display_id: int = 0,
                  whitepoint: str = "D65",
                  gamma: float = 2.2,
                  **kwargs) -> AutomationTask:
        """Create a calibration task."""
        return self.create_task(
            TaskType.CALIBRATE,
            f"Calibrate Display {display_id}",
            parameters={
                "display_id": display_id,
                "whitepoint": whitepoint,
                "gamma": gamma,
                **kwargs,
            }
        )

    def verify(self,
               display_id: int = 0,
               verification_type: str = "colorchecker",
               **kwargs) -> AutomationTask:
        """Create a verification task."""
        return self.create_task(
            TaskType.VERIFY,
            f"Verify Display {display_id}",
            parameters={
                "display_id": display_id,
                "type": verification_type,
                **kwargs,
            }
        )

    def generate_profile(self,
                        output_path: str,
                        **kwargs) -> AutomationTask:
        """Create a profile generation task."""
        return self.create_task(
            TaskType.PROFILE,
            "Generate ICC Profile",
            parameters={
                "output_path": output_path,
                **kwargs,
            }
        )

    def generate_lut(self,
                     lut_path: str,
                     lut_size: int = 33,
                     **kwargs) -> AutomationTask:
        """Create a LUT generation task."""
        return self.create_task(
            TaskType.LUT_GENERATE,
            "Generate 3D LUT",
            parameters={
                "action": "generate",
                "lut_path": lut_path,
                "lut_size": lut_size,
                **kwargs,
            }
        )

    def apply_lut(self,
                  lut_path: str,
                  display_id: int = 0,
                  **kwargs) -> AutomationTask:
        """Create a LUT application task."""
        return self.create_task(
            TaskType.LUT_APPLY,
            "Apply 3D LUT",
            parameters={
                "action": "apply",
                "lut_path": lut_path,
                "display_id": display_id,
                **kwargs,
            }
        )

    def export_report(self,
                      output_path: str,
                      report_type: str = "pdf",
                      **kwargs) -> AutomationTask:
        """Create an export task."""
        return self.create_task(
            TaskType.EXPORT,
            "Export Report",
            parameters={
                "type": "report",
                "output_path": output_path,
                "format": report_type,
                **kwargs,
            }
        )

    def run_script(self,
                   script: str = "",
                   script_path: str = "",
                   **kwargs) -> AutomationTask:
        """Create a custom script task."""
        return self.create_task(
            TaskType.CUSTOM,
            "Run Custom Script",
            parameters={
                "script": script,
                "script_path": script_path,
                **kwargs,
            }
        )

    # =========================================================================
    # Workflow Creation
    # =========================================================================

    def create_workflow(self,
                        name: str,
                        tasks: Optional[List[AutomationTask]] = None,
                        description: str = "") -> Workflow:
        """Create a new workflow."""
        self._workflow_counter += 1
        workflow_id = f"workflow_{self._workflow_counter:04d}"

        workflow = Workflow(
            workflow_id=workflow_id,
            name=name,
            description=description,
            tasks=tasks or [],
        )

        self._workflows[workflow_id] = workflow
        return workflow

    def create_calibration_workflow(self,
                                    display_id: int = 0,
                                    name: str = "Full Calibration") -> Workflow:
        """Create a complete calibration workflow."""
        tasks = [
            self.calibrate(display_id=display_id),
            self.generate_profile(output_path=f"display_{display_id}.icc"),
            self.generate_lut(lut_path=f"display_{display_id}.cube"),
            self.apply_lut(lut_path=f"display_{display_id}.cube", display_id=display_id),
            self.verify(display_id=display_id),
            self.export_report(output_path=f"report_{display_id}.pdf"),
        ]

        # Set dependencies
        for i in range(1, len(tasks)):
            tasks[i].depends_on = [tasks[i-1].task_id]

        return self.create_workflow(name, tasks)

    def create_verification_workflow(self,
                                     display_id: int = 0,
                                     name: str = "Verification") -> Workflow:
        """Create a verification-only workflow."""
        tasks = [
            self.verify(display_id=display_id, verification_type="colorchecker"),
            self.verify(display_id=display_id, verification_type="grayscale"),
            self.export_report(output_path=f"verification_{display_id}.pdf"),
        ]

        tasks[2].depends_on = [tasks[0].task_id, tasks[1].task_id]

        return self.create_workflow(name, tasks)

    # =========================================================================
    # Execution
    # =========================================================================

    def run(self, workflow: Workflow) -> Workflow:
        """Run a workflow synchronously."""
        return asyncio.run(self.engine.execute_workflow(workflow))

    async def run_async(self, workflow: Workflow) -> Workflow:
        """Run a workflow asynchronously."""
        return await self.engine.execute_workflow(workflow)

    def run_task(self, task: AutomationTask) -> TaskResult:
        """Run a single task synchronously."""
        return asyncio.run(self.engine.execute_task(task))

    async def run_task_async(self, task: AutomationTask) -> TaskResult:
        """Run a single task asynchronously."""
        return await self.engine.execute_task(task)

    # =========================================================================
    # Scheduling
    # =========================================================================

    def schedule(self,
                 workflow: Workflow,
                 interval_hours: Optional[float] = None,
                 cron: Optional[str] = None,
                 name: str = "") -> ScheduledTask:
        """Schedule a workflow for recurring execution."""
        schedule_id = f"schedule_{len(self._scheduled_tasks):04d}"

        scheduled = ScheduledTask(
            schedule_id=schedule_id,
            workflow_id=workflow.workflow_id,
            name=name or f"Scheduled: {workflow.name}",
            cron_expression=cron or "",
            interval_seconds=int(interval_hours * 3600) if interval_hours else None,
        )

        if scheduled.interval_seconds:
            scheduled.next_run = datetime.now() + timedelta(seconds=scheduled.interval_seconds)

        self._scheduled_tasks[schedule_id] = scheduled
        return scheduled

    def unschedule(self, schedule_id: str) -> bool:
        """Remove a scheduled task."""
        if schedule_id in self._scheduled_tasks:
            del self._scheduled_tasks[schedule_id]
            return True
        return False

    # =========================================================================
    # Event Handling
    # =========================================================================

    def on_event(self,
                 callback: Callable[[AutomationEvent], None]) -> None:
        """Register an event callback."""
        self.engine.add_event_listener(callback)

    # =========================================================================
    # Persistence
    # =========================================================================

    def save_workflow(self, workflow: Workflow, path: str) -> None:
        """Save workflow definition to JSON."""
        data = {
            "workflow_id": workflow.workflow_id,
            "name": workflow.name,
            "description": workflow.description,
            "tasks": [
                {
                    "task_id": t.task_id,
                    "task_type": t.task_type.name,
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                    "depends_on": t.depends_on,
                    "max_retries": t.max_retries,
                    "timeout": t.timeout,
                }
                for t in workflow.tasks
            ],
            "tags": workflow.tags,
        }

        with open(path, 'w') as f:
            json.dump(data, f, indent=2)

    def load_workflow(self, path: str) -> Workflow:
        """Load workflow definition from JSON."""
        with open(path, 'r') as f:
            data = json.load(f)

        tasks = [
            AutomationTask(
                task_id=t["task_id"],
                task_type=TaskType[t["task_type"]],
                name=t["name"],
                description=t.get("description", ""),
                parameters=t.get("parameters", {}),
                depends_on=t.get("depends_on", []),
                max_retries=t.get("max_retries", 3),
                timeout=t.get("timeout", 300),
            )
            for t in data["tasks"]
        ]

        workflow = Workflow(
            workflow_id=data["workflow_id"],
            name=data["name"],
            description=data.get("description", ""),
            tasks=tasks,
            tags=data.get("tags", []),
        )

        self._workflows[workflow.workflow_id] = workflow
        return workflow


# =============================================================================
# CLI Interface
# =============================================================================

def create_cli_parser():
    """Create command-line argument parser."""
    import argparse

    parser = argparse.ArgumentParser(
        description="Calibrate Pro Automation CLI",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    subparsers = parser.add_subparsers(dest="command", help="Available commands")

    # Calibrate command
    cal_parser = subparsers.add_parser("calibrate", help="Run calibration")
    cal_parser.add_argument("--display", type=int, default=0, help="Display ID")
    cal_parser.add_argument("--whitepoint", default="D65", help="Target whitepoint")
    cal_parser.add_argument("--gamma", type=float, default=2.2, help="Target gamma")

    # Verify command
    ver_parser = subparsers.add_parser("verify", help="Run verification")
    ver_parser.add_argument("--display", type=int, default=0, help="Display ID")
    ver_parser.add_argument("--type", default="colorchecker", help="Verification type")

    # Workflow command
    wf_parser = subparsers.add_parser("workflow", help="Run workflow")
    wf_parser.add_argument("path", help="Path to workflow JSON file")

    # Batch command
    batch_parser = subparsers.add_parser("batch", help="Run batch calibration")
    batch_parser.add_argument("--displays", type=str, help="Comma-separated display IDs")

    return parser


def run_cli(args: Optional[List[str]] = None) -> int:
    """Run CLI interface."""
    parser = create_cli_parser()
    parsed = parser.parse_args(args)

    api = AutomationAPI()

    # Event logger
    def log_event(event: AutomationEvent):
        logger.info(f"[{event.event_type.name}] {event.data}")

    api.on_event(log_event)

    if parsed.command == "calibrate":
        task = api.calibrate(
            display_id=parsed.display,
            whitepoint=parsed.whitepoint,
            gamma=parsed.gamma,
        )
        result = api.run_task(task)
        print(f"Calibration {'succeeded' if result.success else 'failed'}")
        if result.data:
            print(f"Delta E Mean: {result.data.get('delta_e_mean', 'N/A')}")
        return 0 if result.success else 1

    elif parsed.command == "verify":
        task = api.verify(
            display_id=parsed.display,
            verification_type=parsed.type,
        )
        result = api.run_task(task)
        print(f"Verification {'passed' if result.success else 'failed'}")
        return 0 if result.success else 1

    elif parsed.command == "workflow":
        workflow = api.load_workflow(parsed.path)
        result = api.run(workflow)
        print(f"Workflow {result.state.name}")
        if result.errors:
            print(f"Errors: {result.errors}")
        return 0 if result.state == WorkflowState.COMPLETED else 1

    elif parsed.command == "batch":
        display_ids = [int(d) for d in parsed.displays.split(",")]
        for display_id in display_ids:
            workflow = api.create_calibration_workflow(display_id)
            result = api.run(workflow)
            print(f"Display {display_id}: {result.state.name}")
        return 0

    else:
        parser.print_help()
        return 0


# =============================================================================
# Utility Functions
# =============================================================================

def print_workflow_status(workflow: Workflow) -> None:
    """Print workflow execution status."""
    print("\n" + "=" * 60)
    print(f"Workflow: {workflow.name}")
    print("=" * 60)
    print(f"State: {workflow.state.name}")
    print(f"Progress: {workflow.progress:.0%}")
    print()
    print("Tasks:")
    for task in workflow.tasks:
        status_icon = {
            TaskStatus.PENDING: "○",
            TaskStatus.RUNNING: "◐",
            TaskStatus.COMPLETED: "●",
            TaskStatus.FAILED: "✗",
            TaskStatus.SKIPPED: "○",
        }.get(task.status, "?")
        print(f"  {status_icon} {task.name} [{task.status.name}]")
        if task.result and task.result.error:
            print(f"      Error: {task.result.error}")
    print()
    if workflow.errors:
        print(f"Errors: {workflow.errors}")
    print("=" * 60)


# =============================================================================
# Module Test
# =============================================================================

if __name__ == "__main__":
    # Test automation API
    api = AutomationAPI()

    # Event logger
    def log_event(event: AutomationEvent):
        print(f"[EVENT] {event.event_type.name}: {event.data}")

    api.on_event(log_event)

    # Create and run a calibration workflow
    print("Creating calibration workflow...")
    workflow = api.create_calibration_workflow(display_id=1)

    print(f"Workflow: {workflow.name}")
    print(f"Tasks: {len(workflow.tasks)}")

    print("\nRunning workflow...")
    result = api.run(workflow)

    print_workflow_status(result)

    # Run individual task
    print("\n\nRunning individual verification task...")
    task = api.verify(display_id=1, verification_type="grayscale")
    task_result = api.run_task(task)
    print(f"Task result: {task_result}")
