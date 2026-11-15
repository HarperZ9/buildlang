#!/usr/bin/env python3
"""
Calibrate Pro - Advanced Features Demo

This script demonstrates all the advanced calibration features:
1. Uniformity Compensation
2. Ambient Light Adaptation
3. Network/Fleet Calibration
4. 3D LUT Optimization
5. Automation API

Run with: python -m calibrate_pro.advanced.demo
"""

import sys
import numpy as np
from pathlib import Path

# Add parent to path for standalone execution
if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def demo_uniformity():
    """Demonstrate uniformity compensation features."""
    print("\n" + "=" * 70)
    print("UNIFORMITY COMPENSATION DEMO")
    print("=" * 70)

    from calibrate_pro.advanced.uniformity import (
        UniformityGrid, UniformityAnalyzer, UniformityCompensator,
        create_test_measurements, generate_grid_positions
    )

    # Generate measurement positions for 5x5 grid
    print("\n[1] Generating 5x5 measurement grid...")
    positions = generate_grid_positions(UniformityGrid.GRID_5X5, 1920, 1080)
    print(f"    Generated {len(positions)} measurement positions")

    # Create simulated measurements (with typical OLED vignetting)
    print("\n[2] Simulating display measurements...")
    measurements = create_test_measurements(UniformityGrid.GRID_5X5)
    print(f"    Created {len(measurements)} measurements")

    # Analyze uniformity
    print("\n[3] Analyzing uniformity...")
    analyzer = UniformityAnalyzer()
    result = analyzer.analyze(measurements, "Demo Display")

    print(f"\n    Display: {result.display_name}")
    print(f"    Grid: {result.grid_size.name}")
    print(f"    Luminance Range: {result.luminance_min:.1f} - {result.luminance_max:.1f} cd/m2")
    print(f"    Luminance Uniformity: {result.luminance_uniformity:.1f}%")
    print(f"    Color Uniformity (delta_uv): {result.delta_uv_mean:.4f}")
    print(f"    Overall Grade: {result.overall_grade.name}")

    # Generate correction LUT
    print("\n[4] Generating correction LUT...")
    compensator = UniformityCompensator()
    correction = compensator.generate_correction_lut(result)

    print(f"    Correction Mode: {correction.mode.name}")
    print(f"    Luminance Correction Range: {correction.luminance_corrections.min():.3f}x - {correction.luminance_corrections.max():.3f}x")

    print("\n[OK] Uniformity compensation demo complete!")


def demo_ambient_light():
    """Demonstrate ambient light adaptation features."""
    print("\n" + "=" * 70)
    print("AMBIENT LIGHT ADAPTATION DEMO")
    print("=" * 70)

    from calibrate_pro.advanced.ambient_light import (
        AdaptationMode, SimulatedSensor, AdaptationController,
        classify_ambient, condition_to_string, create_default_schedule,
        LUX_THRESHOLDS, PRESET_PROFILES
    )

    # Show lux thresholds
    print("\n[1] Ambient light classification thresholds:")
    for condition, (low, high) in LUX_THRESHOLDS.items():
        high_str = f"{high:.0f}" if high != float('inf') else "inf"
        print(f"    {condition.name}: {low:.0f} - {high_str} lux")

    # Create simulated sensor
    print("\n[2] Simulating ambient light readings...")
    for lux_level in [30, 150, 350, 750, 1500]:
        sensor = SimulatedSensor(base_lux=float(lux_level))
        reading = sensor.read()
        condition = classify_ambient(reading.lux)
        print(f"    {reading.lux:.0f} lux -> {condition_to_string(condition)}")

    # Show preset profiles
    print("\n[3] Display profiles for different conditions:")
    for name, profile in list(PRESET_PROFILES.items())[:4]:
        print(f"    {name}: {profile.luminance} cd/m2, CCT={profile.whitepoint_cct}K, gamma={profile.gamma}")

    # Create controller
    print("\n[4] Adaptation controller with auto-sensor mode...")
    sensor = SimulatedSensor(base_lux=400.0)
    controller = AdaptationController(sensor=sensor, mode=AdaptationMode.AUTO_SENSOR)
    reading = sensor.read()
    controller.process_reading(reading)

    print(f"    Mode: {controller.mode.name}")
    print(f"    Current lux: {controller.state.current_lux:.1f}")
    print(f"    Active profile: {controller.state.active_profile.name if controller.state.active_profile else 'None'}")

    # Show schedule
    print("\n[5] Default circadian schedule:")
    schedule = create_default_schedule()
    for entry in schedule:
        print(f"    {entry.start_time} - {entry.profile_name}")

    print("\n[OK] Ambient light adaptation demo complete!")


def demo_network_calibration():
    """Demonstrate network/fleet calibration features."""
    print("\n" + "=" * 70)
    print("NETWORK/FLEET CALIBRATION DEMO")
    print("=" * 70)

    from calibrate_pro.advanced.network_calibration import (
        NodeStatus, JobStatus, JobType,
        CalibrationServer, CalibrationClient, ProfileSyncManager,
        create_test_nodes
    )

    # Create test nodes
    print("\n[1] Creating fleet of display nodes...")
    nodes = create_test_nodes(5)
    for node in nodes:
        print(f"    {node.display_name}: {node.display_model} @ {node.ip_address} [{node.status.name}]")

    # Create calibration server
    print("\n[2] Starting calibration server...")
    server = CalibrationServer(host='127.0.0.1', port=9999, server_id='demo-server')
    print(f"    Server ID: {server.server_id}")
    print(f"    Endpoint: {server.host}:{server.port}")

    # Register nodes
    print("\n[3] Registering nodes with server...")
    for node in nodes:
        server.register_node(node)
    print(f"    Registered {len(server.nodes)} nodes")

    # Create calibration jobs
    print("\n[4] Creating calibration jobs...")
    job1 = server.create_job(
        job_type=JobType.FULL_CALIBRATION,
        target_nodes=[nodes[0].node_id],
        parameters={'whitepoint': 'D65', 'gamma': 2.2}
    )
    job2 = server.create_job(
        job_type=JobType.VERIFICATION_ONLY,
        target_nodes=[n.node_id for n in nodes[1:3]],
        parameters={'patches': 24}
    )
    print(f"    Job 1: {job1.job_type.name} for {len(job1.target_nodes)} node(s)")
    print(f"    Job 2: {job2.job_type.name} for {len(job2.target_nodes)} node(s)")

    # Profile sync
    print("\n[5] Profile synchronization manager...")
    sync_manager = ProfileSyncManager(server)
    print(f"    Sync mode: {sync_manager.sync_mode.name}")
    print(f"    Pending updates: {sync_manager.sync_state.pending_updates}")

    # Fleet summary
    print("\n[6] Fleet summary:")
    online = sum(1 for n in server.nodes.values() if n.status == NodeStatus.ONLINE)
    print(f"    Total nodes: {len(server.nodes)}")
    print(f"    Online: {online}")
    print(f"    Pending jobs: {len(server.job_queue)}")

    print("\n[OK] Network calibration demo complete!")


def demo_lut_optimization():
    """Demonstrate 3D LUT optimization features."""
    print("\n" + "=" * 70)
    print("3D LUT OPTIMIZATION DEMO")
    print("=" * 70)

    from calibrate_pro.advanced.lut_optimization import (
        SmoothingMethod, GamutMappingMethod, OptimizationGoal,
        LUTOptimizer, analyze_lut_quality,
        smooth_lut_gaussian, smooth_lut_bilateral,
        create_identity_lut, create_test_lut
    )

    # Create LUTs
    print("\n[1] Creating LUTs...")
    identity_lut = create_identity_lut(17)
    test_lut = create_test_lut(17, contrast=1.15, saturation=1.1)
    print(f"    Identity LUT: {identity_lut.shape}")
    print(f"    Test LUT: {test_lut.shape} (contrast=1.15, saturation=1.1)")

    # Analyze quality
    print("\n[2] Analyzing LUT quality...")
    metrics = analyze_lut_quality(test_lut)
    print(f"    Gradient (mean): {metrics.gradient_mean:.4f}")
    print(f"    Gradient (max): {metrics.gradient_max:.4f}")
    print(f"    Out of gamut: {metrics.out_of_gamut_percent:.2%}")
    print(f"    Clipped values: {metrics.clipped_values_percent:.2%}")

    # Smoothing comparison
    print("\n[3] Comparing smoothing methods...")
    gaussian = smooth_lut_gaussian(test_lut, sigma=0.5)
    bilateral = smooth_lut_bilateral(test_lut, sigma_spatial=0.5, sigma_range=0.1)

    gaussian_metrics = analyze_lut_quality(gaussian)
    bilateral_metrics = analyze_lut_quality(bilateral)

    print(f"    Original gradient:  {metrics.gradient_mean:.4f}")
    print(f"    Gaussian gradient:  {gaussian_metrics.gradient_mean:.4f}")
    print(f"    Bilateral gradient: {bilateral_metrics.gradient_mean:.4f}")

    # Optimization
    print("\n[4] Optimizing LUT...")
    optimizer = LUTOptimizer(goal=OptimizationGoal.BALANCED)
    result = optimizer.optimize(test_lut, reference=identity_lut)

    print(f"    Method: {result.method}")
    print(f"    Original Delta E: {result.original_metrics.delta_e_mean:.4f}")
    print(f"    Optimized Delta E: {result.optimized_metrics.delta_e_mean:.4f}")
    print(f"    Improvement: {result.improvement_percent:.1f}%")
    print(f"    Processing time: {result.processing_time:.3f}s")

    # Different optimization goals
    print("\n[5] Optimization goals comparison:")
    for goal in [OptimizationGoal.MIN_DELTA_E, OptimizationGoal.SMOOTH, OptimizationGoal.BALANCED]:
        opt = LUTOptimizer(goal=goal)
        res = opt.optimize(test_lut, reference=identity_lut)
        print(f"    {goal.name}: Delta E = {res.optimized_metrics.delta_e_mean:.4f}, Improvement = {res.improvement_percent:.1f}%")

    print("\n[OK] LUT optimization demo complete!")


def demo_automation():
    """Demonstrate automation API features."""
    print("\n" + "=" * 70)
    print("AUTOMATION API DEMO")
    print("=" * 70)

    from calibrate_pro.advanced.automation import (
        TaskStatus, TaskType, WorkflowState,
        AutomationAPI
    )

    # Create API
    print("\n[1] Creating Automation API...")
    api = AutomationAPI()
    print(f"    Available task handlers:")
    for task_type in api.engine.handlers:
        print(f"      - {task_type.name}")

    # Create custom workflow
    print("\n[2] Creating custom workflow...")
    workflow = api.create_workflow(
        name="Custom Demo Workflow",
        description="Demonstrates workflow creation"
    )
    print(f"    Workflow: {workflow.name}")
    print(f"    ID: {workflow.workflow_id[:8]}...")

    # Add tasks
    print("\n[3] Adding tasks to workflow...")
    task1 = api.create_task("Measure Display", TaskType.CALIBRATE, {'display_id': 0})
    task2 = api.create_task("Generate Profile", TaskType.PROFILE, {'format': 'icc'})
    task3 = api.create_task("Generate LUT", TaskType.LUT_GENERATE, {'size': 33})
    task4 = api.create_task("Verify Results", TaskType.VERIFY, {'patches': 24})

    workflow.tasks.append(task1)
    workflow.tasks.append(task2)
    workflow.tasks.append(task3)
    workflow.tasks.append(task4)

    print(f"    Added {len(workflow.tasks)} tasks")

    # Create pre-built workflows
    print("\n[4] Creating pre-built workflows...")
    cal_workflow = api.create_calibration_workflow(display_id=0)
    ver_workflow = api.create_verification_workflow(display_id=0)

    print(f"    Calibration workflow: {len(cal_workflow.tasks)} tasks")
    for task in cal_workflow.tasks:
        print(f"      - {task.name} ({task.task_type.name})")

    print(f"\n    Verification workflow: {len(ver_workflow.tasks)} tasks")
    for task in ver_workflow.tasks:
        print(f"      - {task.name} ({task.task_type.name})")

    # Event system
    print("\n[5] Event system...")
    events_received = []

    def on_event(event):
        events_received.append(event)

    api.on_event(on_event)
    print("    Event handler registered")

    # Summary
    print("\n[6] Automation summary:")
    print(f"    Task types: {len(api.engine.handlers)}")
    print(f"    Custom workflow tasks: {len(workflow.tasks)}")
    print(f"    Calibration workflow tasks: {len(cal_workflow.tasks)}")
    print(f"    Verification workflow tasks: {len(ver_workflow.tasks)}")

    print("\n[OK] Automation API demo complete!")


def main():
    """Run all demos."""
    print("=" * 70)
    print("CALIBRATE PRO - ADVANCED FEATURES DEMONSTRATION")
    print("=" * 70)
    print("\nThis demo showcases all advanced calibration features.")

    try:
        demo_uniformity()
        demo_ambient_light()
        demo_network_calibration()
        demo_lut_optimization()
        demo_automation()

        print("\n" + "=" * 70)
        print("ALL DEMOS COMPLETED SUCCESSFULLY!")
        print("=" * 70)
        print("\nAdvanced features demonstrated:")
        print("  [x] Uniformity Compensation - 5x5/9x9 grid measurement and correction")
        print("  [x] Ambient Light Adaptation - Sensor integration and circadian rhythm")
        print("  [x] Network Calibration - Fleet management and profile sync")
        print("  [x] LUT Optimization - Smoothing, gamut mapping, delta E minimization")
        print("  [x] Automation API - Workflows, tasks, and batch processing")
        print("\n")

    except Exception as e:
        print(f"\n[ERROR] Demo failed: {e}")
        import traceback
        traceback.print_exc()
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
