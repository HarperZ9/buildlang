"""
Calibrate Pro - Advanced Features Module

This module provides advanced calibration features that surpass competition:
- Uniformity compensation for display non-uniformities
- Ambient light adaptation with sensor integration
- Network/fleet calibration for studio environments
- 3D LUT optimization with perceptual smoothing
- Automation API for batch workflows and scripting

Example usage:
    from calibrate_pro.advanced import (
        # Uniformity
        UniformityAnalyzer, UniformityCompensator,

        # Ambient Light
        AdaptationController, AmbientSensor,

        # Network Calibration
        CalibrationServer, CalibrationClient,

        # LUT Optimization
        LUTOptimizer,

        # Automation
        AutomationAPI, Workflow, WorkflowEngine
    )
"""

# =============================================================================
# Uniformity Compensation
# =============================================================================
from .uniformity import (
    # Enums
    UniformityGrid,
    UniformityGrade,
    CompensationMode,

    # Data Classes
    UniformityMeasurement,
    UniformityRegion,
    UniformityResult,
    UniformityCorrectionLUT,

    # Main Classes
    UniformityAnalyzer,
    UniformityCompensator,

    # Functions
    generate_grid_positions,
    grade_from_uniformity,
    grade_to_string,
    create_test_measurements,
    print_uniformity_summary,
)

# =============================================================================
# Ambient Light Adaptation
# =============================================================================
from .ambient_light import (
    # Enums
    AdaptationMode,
    ProfileType,
    AmbientCondition,

    # Data Classes
    AmbientReading,
    DisplayProfile,
    CircadianSettings,
    AdaptationState,
    ScheduleEntry,

    # Main Classes
    AmbientSensor,
    WindowsLightSensor,
    SimulatedSensor,
    AdaptationController,

    # Functions / Constants
    LUX_THRESHOLDS,
    PRESET_PROFILES,
    classify_ambient,
    condition_to_string,
    create_default_schedule,
    print_adaptation_status,
)

# =============================================================================
# Network/Fleet Calibration
# =============================================================================
from .network_calibration import (
    # Enums
    NodeStatus,
    JobStatus,
    JobType,
    MessageType,
    ProfileSyncMode,

    # Data Classes
    DisplayNode,
    CalibrationJob,
    ProfilePackage,
    NetworkMessage,
    SyncState,

    # Main Classes
    CalibrationServer,
    CalibrationClient,
    ProfileSyncManager,

    # Functions
    create_test_nodes,
    print_fleet_status,
)

# =============================================================================
# 3D LUT Optimization
# =============================================================================
from .lut_optimization import (
    # Enums
    SmoothingMethod,
    GamutMappingMethod,
    OptimizationGoal,

    # Data Classes
    LUTQualityMetrics,
    OptimizationResult,
    SmoothingConfig,
    GamutConfig,

    # Main Classes
    LUTOptimizer,

    # Functions
    analyze_lut_quality,
    analyze_interpolation_quality,
    smooth_lut_gaussian,
    smooth_lut_bilateral,
    smooth_lut_perceptual,
    map_gamut_clip,
    map_gamut_compress,
    map_gamut_perceptual,
    create_identity_lut,
    create_test_lut,
    print_optimization_summary,
)

# =============================================================================
# Automation API
# =============================================================================
from .automation import (
    # Enums
    TaskStatus,
    TaskType,
    EventType,
    WorkflowState,

    # Data Classes
    AutomationTask,
    TaskResult,
    Workflow,
    ScheduledTask,
    AutomationEvent,

    # Main Classes
    TaskHandler,
    WorkflowEngine,
    AutomationAPI,

    # Functions
    run_cli,
    create_cli_parser,
    print_workflow_status,
)

# =============================================================================
# Convenience Aliases
# =============================================================================

# Common analyzer/controller shortcuts
Uniformity = UniformityAnalyzer
AmbientLight = AdaptationController
FleetCalibration = CalibrationServer
LUTOptimize = LUTOptimizer
Automation = AutomationAPI

# =============================================================================
# Module Info
# =============================================================================

__all__ = [
    # Uniformity Compensation
    "UniformityGrid",
    "UniformityGrade",
    "CompensationMode",
    "UniformityMeasurement",
    "UniformityRegion",
    "UniformityResult",
    "UniformityCorrectionLUT",
    "UniformityAnalyzer",
    "UniformityCompensator",
    "generate_grid_positions",
    "grade_from_uniformity",
    "grade_to_string",
    "create_test_measurements",
    "print_uniformity_summary",

    # Ambient Light Adaptation
    "AdaptationMode",
    "ProfileType",
    "AmbientCondition",
    "AmbientReading",
    "DisplayProfile",
    "CircadianSettings",
    "AdaptationState",
    "ScheduleEntry",
    "AmbientSensor",
    "WindowsLightSensor",
    "SimulatedSensor",
    "AdaptationController",
    "LUX_THRESHOLDS",
    "PRESET_PROFILES",
    "classify_ambient",
    "condition_to_string",
    "create_default_schedule",
    "print_adaptation_status",

    # Network/Fleet Calibration
    "NodeStatus",
    "JobStatus",
    "JobType",
    "MessageType",
    "ProfileSyncMode",
    "DisplayNode",
    "CalibrationJob",
    "ProfilePackage",
    "NetworkMessage",
    "SyncState",
    "CalibrationServer",
    "CalibrationClient",
    "ProfileSyncManager",
    "create_test_nodes",
    "print_fleet_status",

    # 3D LUT Optimization
    "SmoothingMethod",
    "GamutMappingMethod",
    "OptimizationGoal",
    "LUTQualityMetrics",
    "OptimizationResult",
    "SmoothingConfig",
    "GamutConfig",
    "LUTOptimizer",
    "analyze_lut_quality",
    "analyze_interpolation_quality",
    "smooth_lut_gaussian",
    "smooth_lut_bilateral",
    "smooth_lut_perceptual",
    "map_gamut_clip",
    "map_gamut_compress",
    "map_gamut_perceptual",
    "create_identity_lut",
    "create_test_lut",
    "print_optimization_summary",

    # Automation API
    "TaskStatus",
    "TaskType",
    "EventType",
    "WorkflowState",
    "AutomationTask",
    "TaskResult",
    "Workflow",
    "ScheduledTask",
    "AutomationEvent",
    "TaskHandler",
    "WorkflowEngine",
    "AutomationAPI",
    "run_cli",
    "create_cli_parser",
    "print_workflow_status",

    # Convenience Aliases
    "Uniformity",
    "AmbientLight",
    "FleetCalibration",
    "LUTOptimize",
    "Automation",
]

__version__ = "1.0.0"
__author__ = "Calibrate Pro Team"
