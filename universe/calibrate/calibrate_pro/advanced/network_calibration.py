"""
Network Calibration Module

Provides remote and fleet calibration capabilities:
- Remote display calibration over network
- Centralized profile management
- Fleet calibration for studios
- Profile synchronization
- Calibration job scheduling
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Optional, Callable, Any
from datetime import datetime
from pathlib import Path
import json
import hashlib
import uuid
import asyncio
import socket
import struct
import threading
from concurrent.futures import ThreadPoolExecutor

# =============================================================================
# Enums
# =============================================================================

class NodeStatus(Enum):
    """Calibration node status."""
    OFFLINE = auto()
    ONLINE = auto()
    BUSY = auto()
    CALIBRATING = auto()
    ERROR = auto()


class JobStatus(Enum):
    """Calibration job status."""
    PENDING = auto()
    QUEUED = auto()
    RUNNING = auto()
    COMPLETED = auto()
    FAILED = auto()
    CANCELLED = auto()


class JobType(Enum):
    """Type of calibration job."""
    FULL_CALIBRATION = auto()
    VERIFICATION_ONLY = auto()
    PROFILE_APPLY = auto()
    LUT_APPLY = auto()
    MEASUREMENT = auto()


class ProfileSyncMode(Enum):
    """Profile synchronization mode."""
    PUSH = auto()       # Server pushes to clients
    PULL = auto()       # Clients pull from server
    BIDIRECTIONAL = auto()


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class DisplayNode:
    """Remote display node information."""
    node_id: str
    hostname: str
    ip_address: str
    port: int = 5678

    # Display info
    display_name: str = ""
    display_model: str = ""
    display_serial: str = ""

    # Status
    status: NodeStatus = NodeStatus.OFFLINE
    last_seen: Optional[datetime] = None
    last_calibration: Optional[datetime] = None

    # Capabilities
    has_colorimeter: bool = False
    colorimeter_model: str = ""

    # Current profile
    active_profile: str = ""
    active_lut: str = ""

    # Metadata
    tags: list[str] = field(default_factory=list)
    group: str = ""
    location: str = ""

    @property
    def is_available(self) -> bool:
        return self.status in (NodeStatus.ONLINE,)


@dataclass
class CalibrationJob:
    """Calibration job definition."""
    job_id: str
    job_type: JobType
    target_nodes: list[str]     # List of node IDs

    # Job parameters
    parameters: dict = field(default_factory=dict)

    # Status
    status: JobStatus = JobStatus.PENDING
    progress: float = 0.0
    current_node: str = ""

    # Timing
    created_at: datetime = field(default_factory=datetime.now)
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None

    # Results
    results: dict[str, Any] = field(default_factory=dict)
    errors: dict[str, str] = field(default_factory=dict)

    # Metadata
    created_by: str = ""
    priority: int = 0


@dataclass
class ProfilePackage:
    """Distributable calibration profile package."""
    package_id: str
    name: str
    version: str
    description: str = ""

    # Contents
    icc_profile: Optional[bytes] = None
    lut_3d: Optional[bytes] = None
    lut_1d: Optional[bytes] = None
    calibration_data: dict = field(default_factory=dict)

    # Metadata
    created_at: datetime = field(default_factory=datetime.now)
    target_display_model: str = ""
    checksum: str = ""

    def calculate_checksum(self) -> str:
        """Calculate package checksum."""
        data = b""
        if self.icc_profile:
            data += self.icc_profile
        if self.lut_3d:
            data += self.lut_3d
        if self.lut_1d:
            data += self.lut_1d
        self.checksum = hashlib.sha256(data).hexdigest()
        return self.checksum


@dataclass
class SyncState:
    """Profile synchronization state."""
    last_sync: Optional[datetime] = None
    pending_updates: int = 0
    sync_errors: list[str] = field(default_factory=list)
    synced_profiles: list[str] = field(default_factory=list)


# =============================================================================
# Network Protocol Messages
# =============================================================================

class MessageType(Enum):
    """Network message types."""
    PING = 0
    PONG = 1
    STATUS_REQUEST = 2
    STATUS_RESPONSE = 3
    CALIBRATION_START = 4
    CALIBRATION_PROGRESS = 5
    CALIBRATION_COMPLETE = 6
    CALIBRATION_ERROR = 7
    PROFILE_PUSH = 8
    PROFILE_PULL_REQUEST = 9
    PROFILE_DATA = 10
    LUT_PUSH = 11
    MEASUREMENT_REQUEST = 12
    MEASUREMENT_RESULT = 13
    COMMAND = 14
    COMMAND_RESPONSE = 15


@dataclass
class NetworkMessage:
    """Network protocol message."""
    msg_type: MessageType
    sender_id: str
    payload: dict = field(default_factory=dict)
    timestamp: datetime = field(default_factory=datetime.now)

    def to_bytes(self) -> bytes:
        """Serialize message to bytes."""
        data = {
            "type": self.msg_type.value,
            "sender": self.sender_id,
            "payload": self.payload,
            "timestamp": self.timestamp.isoformat(),
        }
        json_data = json.dumps(data).encode('utf-8')
        # Prefix with length
        return struct.pack('>I', len(json_data)) + json_data

    @classmethod
    def from_bytes(cls, data: bytes) -> 'NetworkMessage':
        """Deserialize message from bytes."""
        # Skip length prefix
        json_data = data[4:].decode('utf-8')
        obj = json.loads(json_data)
        return cls(
            msg_type=MessageType(obj["type"]),
            sender_id=obj["sender"],
            payload=obj.get("payload", {}),
            timestamp=datetime.fromisoformat(obj["timestamp"]),
        )


# =============================================================================
# Calibration Server
# =============================================================================

class CalibrationServer:
    """
    Central calibration server for fleet management.

    Manages multiple display nodes, distributes calibration jobs,
    and synchronizes profiles across the network.
    """

    def __init__(self,
                 host: str = "0.0.0.0",
                 port: int = 5678,
                 server_id: Optional[str] = None):
        """
        Initialize calibration server.

        Args:
            host: Server bind address
            port: Server port
            server_id: Unique server identifier
        """
        self.host = host
        self.port = port
        self.server_id = server_id or str(uuid.uuid4())[:8]

        # Node registry
        self.nodes: dict[str, DisplayNode] = {}

        # Job queue
        self.jobs: dict[str, CalibrationJob] = {}
        self.job_queue: list[str] = []

        # Profile repository
        self.profiles: dict[str, ProfilePackage] = {}

        # State
        self._running = False
        self._server_socket: Optional[socket.socket] = None
        self._executor = ThreadPoolExecutor(max_workers=10)

        # Callbacks
        self._node_callbacks: list[Callable[[DisplayNode, str], None]] = []
        self._job_callbacks: list[Callable[[CalibrationJob], None]] = []

    # =========================================================================
    # Node Management
    # =========================================================================

    def register_node(self, node: DisplayNode) -> None:
        """Register a display node."""
        self.nodes[node.node_id] = node
        self._notify_node_change(node, "registered")

    def unregister_node(self, node_id: str) -> None:
        """Unregister a display node."""
        if node_id in self.nodes:
            node = self.nodes.pop(node_id)
            self._notify_node_change(node, "unregistered")

    def get_node(self, node_id: str) -> Optional[DisplayNode]:
        """Get node by ID."""
        return self.nodes.get(node_id)

    def get_nodes_by_group(self, group: str) -> list[DisplayNode]:
        """Get all nodes in a group."""
        return [n for n in self.nodes.values() if n.group == group]

    def get_available_nodes(self) -> list[DisplayNode]:
        """Get all available nodes."""
        return [n for n in self.nodes.values() if n.is_available]

    def update_node_status(self, node_id: str, status: NodeStatus) -> None:
        """Update node status."""
        if node_id in self.nodes:
            self.nodes[node_id].status = status
            self.nodes[node_id].last_seen = datetime.now()
            self._notify_node_change(self.nodes[node_id], "status_changed")

    # =========================================================================
    # Job Management
    # =========================================================================

    def create_job(self,
                   job_type: JobType,
                   target_nodes: list[str],
                   parameters: Optional[dict] = None,
                   priority: int = 0,
                   created_by: str = "") -> CalibrationJob:
        """
        Create a new calibration job.

        Args:
            job_type: Type of calibration job
            target_nodes: List of target node IDs
            parameters: Job parameters
            priority: Job priority (higher = more urgent)
            created_by: Creator identifier

        Returns:
            Created CalibrationJob
        """
        job = CalibrationJob(
            job_id=str(uuid.uuid4())[:8],
            job_type=job_type,
            target_nodes=target_nodes,
            parameters=parameters or {},
            priority=priority,
            created_by=created_by,
        )

        self.jobs[job.job_id] = job
        self._insert_job_by_priority(job.job_id)
        self._notify_job_change(job)

        return job

    def cancel_job(self, job_id: str) -> bool:
        """Cancel a pending or running job."""
        if job_id not in self.jobs:
            return False

        job = self.jobs[job_id]
        if job.status in (JobStatus.COMPLETED, JobStatus.CANCELLED):
            return False

        job.status = JobStatus.CANCELLED
        if job_id in self.job_queue:
            self.job_queue.remove(job_id)

        self._notify_job_change(job)
        return True

    def get_job(self, job_id: str) -> Optional[CalibrationJob]:
        """Get job by ID."""
        return self.jobs.get(job_id)

    def get_pending_jobs(self) -> list[CalibrationJob]:
        """Get all pending jobs."""
        return [self.jobs[jid] for jid in self.job_queue if jid in self.jobs]

    def _insert_job_by_priority(self, job_id: str) -> None:
        """Insert job into queue by priority."""
        job = self.jobs[job_id]
        for i, existing_id in enumerate(self.job_queue):
            if self.jobs[existing_id].priority < job.priority:
                self.job_queue.insert(i, job_id)
                return
        self.job_queue.append(job_id)

    # =========================================================================
    # Profile Distribution
    # =========================================================================

    def add_profile(self, package: ProfilePackage) -> None:
        """Add profile package to repository."""
        package.calculate_checksum()
        self.profiles[package.package_id] = package

    def get_profile(self, package_id: str) -> Optional[ProfilePackage]:
        """Get profile package by ID."""
        return self.profiles.get(package_id)

    def push_profile_to_nodes(self,
                              package_id: str,
                              node_ids: list[str]) -> dict[str, bool]:
        """
        Push profile package to specified nodes.

        Returns dict mapping node_id to success status.
        """
        results: dict[str, bool] = {}
        package = self.get_profile(package_id)

        if not package:
            return {nid: False for nid in node_ids}

        for node_id in node_ids:
            node = self.get_node(node_id)
            if not node or not node.is_available:
                results[node_id] = False
                continue

            # Send profile to node
            success = self._send_profile_to_node(node, package)
            results[node_id] = success

        return results

    def _send_profile_to_node(self, node: DisplayNode, package: ProfilePackage) -> bool:
        """Send profile package to a node."""
        try:
            msg = NetworkMessage(
                msg_type=MessageType.PROFILE_PUSH,
                sender_id=self.server_id,
                payload={
                    "package_id": package.package_id,
                    "name": package.name,
                    "version": package.version,
                    "checksum": package.checksum,
                    "has_icc": package.icc_profile is not None,
                    "has_lut_3d": package.lut_3d is not None,
                    "calibration_data": package.calibration_data,
                }
            )
            # Actual send would go here
            return True
        except Exception:
            return False

    # =========================================================================
    # Fleet Calibration
    # =========================================================================

    def calibrate_fleet(self,
                        group: Optional[str] = None,
                        parameters: Optional[dict] = None) -> CalibrationJob:
        """
        Create calibration job for entire fleet or group.

        Args:
            group: Optional group name to filter nodes
            parameters: Calibration parameters

        Returns:
            Created CalibrationJob
        """
        if group:
            nodes = self.get_nodes_by_group(group)
        else:
            nodes = list(self.nodes.values())

        available_nodes = [n.node_id for n in nodes if n.is_available]

        return self.create_job(
            job_type=JobType.FULL_CALIBRATION,
            target_nodes=available_nodes,
            parameters=parameters or {},
            priority=10,
            created_by="fleet_manager",
        )

    def verify_fleet(self, group: Optional[str] = None) -> CalibrationJob:
        """Create verification job for fleet."""
        if group:
            nodes = self.get_nodes_by_group(group)
        else:
            nodes = list(self.nodes.values())

        available_nodes = [n.node_id for n in nodes if n.is_available]

        return self.create_job(
            job_type=JobType.VERIFICATION_ONLY,
            target_nodes=available_nodes,
            priority=5,
            created_by="fleet_manager",
        )

    def apply_profile_to_fleet(self,
                               package_id: str,
                               group: Optional[str] = None) -> CalibrationJob:
        """Apply profile to entire fleet or group."""
        if group:
            nodes = self.get_nodes_by_group(group)
        else:
            nodes = list(self.nodes.values())

        available_nodes = [n.node_id for n in nodes if n.is_available]

        return self.create_job(
            job_type=JobType.PROFILE_APPLY,
            target_nodes=available_nodes,
            parameters={"package_id": package_id},
            priority=8,
            created_by="fleet_manager",
        )

    # =========================================================================
    # Job Execution
    # =========================================================================

    async def process_jobs(self) -> None:
        """Process pending jobs in the queue."""
        while self._running and self.job_queue:
            job_id = self.job_queue[0]
            job = self.jobs.get(job_id)

            if not job:
                self.job_queue.pop(0)
                continue

            if job.status != JobStatus.PENDING:
                self.job_queue.pop(0)
                continue

            # Start job
            job.status = JobStatus.RUNNING
            job.started_at = datetime.now()
            self._notify_job_change(job)

            # Process each node
            total_nodes = len(job.target_nodes)
            for i, node_id in enumerate(job.target_nodes):
                job.current_node = node_id
                job.progress = i / total_nodes
                self._notify_job_change(job)

                node = self.get_node(node_id)
                if not node or not node.is_available:
                    job.errors[node_id] = "Node unavailable"
                    continue

                # Execute job on node
                try:
                    result = await self._execute_job_on_node(job, node)
                    job.results[node_id] = result
                except Exception as e:
                    job.errors[node_id] = str(e)

            # Complete job
            job.status = JobStatus.COMPLETED if not job.errors else JobStatus.FAILED
            job.completed_at = datetime.now()
            job.progress = 1.0
            self.job_queue.pop(0)
            self._notify_job_change(job)

    async def _execute_job_on_node(self,
                                   job: CalibrationJob,
                                   node: DisplayNode) -> dict:
        """Execute job on a specific node."""
        self.update_node_status(node.node_id, NodeStatus.CALIBRATING)

        try:
            if job.job_type == JobType.FULL_CALIBRATION:
                result = await self._run_calibration(node, job.parameters)
            elif job.job_type == JobType.VERIFICATION_ONLY:
                result = await self._run_verification(node, job.parameters)
            elif job.job_type == JobType.PROFILE_APPLY:
                result = await self._apply_profile(node, job.parameters)
            elif job.job_type == JobType.LUT_APPLY:
                result = await self._apply_lut(node, job.parameters)
            elif job.job_type == JobType.MEASUREMENT:
                result = await self._run_measurement(node, job.parameters)
            else:
                result = {"status": "unknown_job_type"}

            return result
        finally:
            self.update_node_status(node.node_id, NodeStatus.ONLINE)

    async def _run_calibration(self, node: DisplayNode, params: dict) -> dict:
        """Run full calibration on node."""
        # This would send calibration commands to the remote node
        await asyncio.sleep(0.1)  # Placeholder
        return {"status": "completed", "delta_e_mean": 1.2}

    async def _run_verification(self, node: DisplayNode, params: dict) -> dict:
        """Run verification on node."""
        await asyncio.sleep(0.1)
        return {"status": "completed", "passed": True}

    async def _apply_profile(self, node: DisplayNode, params: dict) -> dict:
        """Apply profile to node."""
        await asyncio.sleep(0.1)
        return {"status": "applied"}

    async def _apply_lut(self, node: DisplayNode, params: dict) -> dict:
        """Apply LUT to node."""
        await asyncio.sleep(0.1)
        return {"status": "applied"}

    async def _run_measurement(self, node: DisplayNode, params: dict) -> dict:
        """Run measurement on node."""
        await asyncio.sleep(0.1)
        return {"status": "completed", "measurements": []}

    # =========================================================================
    # Callbacks
    # =========================================================================

    def add_node_callback(self, callback: Callable[[DisplayNode, str], None]) -> None:
        """Add callback for node changes."""
        self._node_callbacks.append(callback)

    def add_job_callback(self, callback: Callable[[CalibrationJob], None]) -> None:
        """Add callback for job changes."""
        self._job_callbacks.append(callback)

    def _notify_node_change(self, node: DisplayNode, event: str) -> None:
        """Notify node change callbacks."""
        for callback in self._node_callbacks:
            try:
                callback(node, event)
            except Exception:
                pass

    def _notify_job_change(self, job: CalibrationJob) -> None:
        """Notify job change callbacks."""
        for callback in self._job_callbacks:
            try:
                callback(job)
            except Exception:
                pass

    # =========================================================================
    # Server Control
    # =========================================================================

    def start(self) -> None:
        """Start the calibration server."""
        self._running = True
        # Server socket setup would go here

    def stop(self) -> None:
        """Stop the calibration server."""
        self._running = False
        if self._server_socket:
            self._server_socket.close()
        self._executor.shutdown(wait=True)


# =============================================================================
# Calibration Client
# =============================================================================

class CalibrationClient:
    """
    Calibration client for remote nodes.

    Connects to a calibration server and executes calibration commands.
    """

    def __init__(self,
                 server_host: str,
                 server_port: int = 5678,
                 node_id: Optional[str] = None):
        """
        Initialize calibration client.

        Args:
            server_host: Server hostname/IP
            server_port: Server port
            node_id: Unique node identifier
        """
        self.server_host = server_host
        self.server_port = server_port
        self.node_id = node_id or str(uuid.uuid4())[:8]

        # Local state
        self.node_info = DisplayNode(
            node_id=self.node_id,
            hostname=socket.gethostname(),
            ip_address=self._get_local_ip(),
        )

        self._connected = False
        self._socket: Optional[socket.socket] = None

        # Callbacks
        self._command_handlers: dict[MessageType, Callable] = {}

    def _get_local_ip(self) -> str:
        """Get local IP address."""
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.connect(("8.8.8.8", 80))
            ip = s.getsockname()[0]
            s.close()
            return ip
        except Exception:
            return "127.0.0.1"

    def connect(self) -> bool:
        """Connect to calibration server."""
        try:
            self._socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self._socket.connect((self.server_host, self.server_port))
            self._connected = True

            # Send registration
            self._send_registration()
            return True
        except Exception:
            self._connected = False
            return False

    def disconnect(self) -> None:
        """Disconnect from server."""
        self._connected = False
        if self._socket:
            self._socket.close()

    def _send_registration(self) -> None:
        """Send node registration to server."""
        msg = NetworkMessage(
            msg_type=MessageType.STATUS_RESPONSE,
            sender_id=self.node_id,
            payload={
                "hostname": self.node_info.hostname,
                "ip_address": self.node_info.ip_address,
                "display_name": self.node_info.display_name,
                "display_model": self.node_info.display_model,
                "has_colorimeter": self.node_info.has_colorimeter,
            }
        )
        self._send_message(msg)

    def _send_message(self, msg: NetworkMessage) -> None:
        """Send message to server."""
        if self._socket and self._connected:
            self._socket.send(msg.to_bytes())

    def send_status(self) -> None:
        """Send current status to server."""
        msg = NetworkMessage(
            msg_type=MessageType.STATUS_RESPONSE,
            sender_id=self.node_id,
            payload={
                "status": self.node_info.status.name,
                "active_profile": self.node_info.active_profile,
                "active_lut": self.node_info.active_lut,
            }
        )
        self._send_message(msg)

    def register_handler(self,
                        msg_type: MessageType,
                        handler: Callable[[NetworkMessage], None]) -> None:
        """Register handler for message type."""
        self._command_handlers[msg_type] = handler

    def process_message(self, msg: NetworkMessage) -> None:
        """Process received message."""
        handler = self._command_handlers.get(msg.msg_type)
        if handler:
            handler(msg)


# =============================================================================
# Profile Sync Manager
# =============================================================================

class ProfileSyncManager:
    """
    Manages profile synchronization across the fleet.

    Handles version control, conflict resolution, and distribution.
    """

    def __init__(self, server: CalibrationServer):
        """
        Initialize sync manager.

        Args:
            server: Calibration server instance
        """
        self.server = server
        self.sync_mode = ProfileSyncMode.PUSH
        self.sync_state = SyncState()

    def sync_all(self) -> SyncState:
        """Synchronize all profiles to all nodes."""
        self.sync_state.pending_updates = 0
        self.sync_state.sync_errors.clear()
        self.sync_state.synced_profiles.clear()

        for package_id, package in self.server.profiles.items():
            available_nodes = [n.node_id for n in self.server.get_available_nodes()]

            if not available_nodes:
                continue

            results = self.server.push_profile_to_nodes(package_id, available_nodes)

            for node_id, success in results.items():
                if success:
                    if package_id not in self.sync_state.synced_profiles:
                        self.sync_state.synced_profiles.append(package_id)
                else:
                    self.sync_state.sync_errors.append(
                        f"Failed to sync {package.name} to {node_id}"
                    )

        self.sync_state.last_sync = datetime.now()
        return self.sync_state

    def sync_to_group(self, group: str) -> SyncState:
        """Synchronize profiles to a specific group."""
        nodes = self.server.get_nodes_by_group(group)
        node_ids = [n.node_id for n in nodes if n.is_available]

        for package_id in self.server.profiles:
            self.server.push_profile_to_nodes(package_id, node_ids)

        self.sync_state.last_sync = datetime.now()
        return self.sync_state


# =============================================================================
# Utility Functions
# =============================================================================

def create_test_nodes(count: int = 5) -> list[DisplayNode]:
    """Create test display nodes."""
    nodes = []
    for i in range(count):
        node = DisplayNode(
            node_id=f"node_{i:03d}",
            hostname=f"display-{i:03d}",
            ip_address=f"192.168.1.{100 + i}",
            display_name=f"Monitor {i + 1}",
            display_model="Professional Display",
            status=NodeStatus.ONLINE,
            last_seen=datetime.now(),
            group=f"group_{i % 3}",
            location=f"Room {i // 2 + 1}",
        )
        nodes.append(node)
    return nodes


def print_fleet_status(server: CalibrationServer) -> None:
    """Print fleet status summary."""
    print("\n" + "=" * 60)
    print("Fleet Calibration Status")
    print("=" * 60)
    print(f"Server ID: {server.server_id}")
    print(f"Total Nodes: {len(server.nodes)}")
    print()

    # Status summary
    status_counts = {}
    for node in server.nodes.values():
        status_counts[node.status] = status_counts.get(node.status, 0) + 1

    print("Node Status:")
    for status, count in status_counts.items():
        print(f"  {status.name}: {count}")
    print()

    # Group summary
    groups: dict[str, list[DisplayNode]] = {}
    for node in server.nodes.values():
        if node.group not in groups:
            groups[node.group] = []
        groups[node.group].append(node)

    if groups:
        print("Groups:")
        for group, nodes in groups.items():
            online = sum(1 for n in nodes if n.status == NodeStatus.ONLINE)
            print(f"  {group or 'ungrouped'}: {len(nodes)} nodes ({online} online)")
    print()

    # Jobs
    pending = len([j for j in server.jobs.values() if j.status == JobStatus.PENDING])
    running = len([j for j in server.jobs.values() if j.status == JobStatus.RUNNING])
    print(f"Jobs: {pending} pending, {running} running")

    # Profiles
    print(f"Profiles: {len(server.profiles)}")
    print("=" * 60)


# =============================================================================
# Module Test
# =============================================================================

if __name__ == "__main__":
    # Test calibration server
    server = CalibrationServer(server_id="test_server")

    # Register test nodes
    for node in create_test_nodes(5):
        server.register_node(node)

    # Create a test profile package
    package = ProfilePackage(
        package_id="pkg_001",
        name="Standard sRGB",
        version="1.0",
        description="Standard sRGB calibration profile",
        calibration_data={
            "whitepoint": "D65",
            "gamma": 2.2,
            "gamut": "sRGB",
        }
    )
    server.add_profile(package)

    # Create fleet calibration job
    job = server.calibrate_fleet(group="group_0")
    print(f"Created calibration job: {job.job_id}")
    print(f"  Target nodes: {job.target_nodes}")
    print(f"  Status: {job.status.name}")

    # Print fleet status
    print_fleet_status(server)

    # Simulate sync
    sync_manager = ProfileSyncManager(server)
    sync_state = sync_manager.sync_all()
    print(f"\nSync completed: {len(sync_state.synced_profiles)} profiles synced")
    if sync_state.sync_errors:
        print(f"Errors: {sync_state.sync_errors}")
