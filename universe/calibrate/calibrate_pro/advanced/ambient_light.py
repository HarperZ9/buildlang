"""
Ambient Light Adaptation Module

Provides automatic display adaptation based on ambient conditions:
- Ambient light sensor integration
- Automatic profile switching
- Time-based profile scheduling
- Circadian rhythm adaptation
- Viewing condition presets
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Optional, Callable, Any
from datetime import datetime, time, timedelta
from pathlib import Path
import json
import threading
import time as time_module

# =============================================================================
# Enums
# =============================================================================

class AmbientCondition(Enum):
    """Ambient lighting conditions."""
    DARK = auto()           # <50 lux (home theater, night)
    DIM = auto()            # 50-200 lux (dim room, evening)
    OFFICE = auto()         # 200-500 lux (typical office)
    BRIGHT = auto()         # 500-1000 lux (bright office, daylight)
    VERY_BRIGHT = auto()    # >1000 lux (direct sunlight)


class AdaptationMode(Enum):
    """Adaptation behavior mode."""
    MANUAL = auto()         # User manually switches profiles
    AUTO_SENSOR = auto()    # Automatic based on light sensor
    SCHEDULED = auto()      # Time-based scheduling
    CIRCADIAN = auto()      # Follows circadian rhythm
    HYBRID = auto()         # Sensor + schedule combined


class ProfileType(Enum):
    """Display profile type for different conditions."""
    DAY = auto()            # Daytime/bright conditions
    NIGHT = auto()          # Nighttime/dark conditions
    HDR = auto()            # HDR content
    SDR = auto()            # SDR content
    CINEMA = auto()         # Dark room movie watching
    PHOTO = auto()          # Photo editing (D50/D65)
    VIDEO = auto()          # Video editing (D65)
    GAMING = auto()         # Gaming optimized
    READING = auto()        # Reduced blue light
    CUSTOM = auto()         # User-defined


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class AmbientReading:
    """Single ambient light sensor reading."""
    timestamp: datetime
    lux: float              # Illuminance in lux
    cct: Optional[float] = None  # Color temperature if available
    condition: AmbientCondition = AmbientCondition.OFFICE

    def __post_init__(self):
        self.condition = classify_ambient(self.lux)


@dataclass
class DisplayProfile:
    """Display calibration profile."""
    name: str
    profile_type: ProfileType

    # Target parameters
    whitepoint_cct: float = 6500    # Target CCT in Kelvin
    gamma: float = 2.2              # Target gamma
    luminance: float = 120          # Target peak luminance cd/m²
    black_level: float = 0.5        # Target black level cd/m²

    # ICC profile path
    icc_path: Optional[str] = None

    # 3D LUT path
    lut_path: Optional[str] = None

    # Conditions when this profile should be active
    min_lux: float = 0
    max_lux: float = float('inf')
    start_time: Optional[time] = None
    end_time: Optional[time] = None

    # Blue light filter (0-100%)
    blue_light_filter: float = 0

    # Priority (higher = more preferred when conditions match)
    priority: int = 0

    @property
    def is_time_based(self) -> bool:
        return self.start_time is not None and self.end_time is not None


@dataclass
class ScheduleEntry:
    """Time-based profile schedule entry."""
    start_time: time
    end_time: time
    profile_name: str
    days: list[int] = field(default_factory=lambda: list(range(7)))  # 0=Monday

    def is_active(self, dt: datetime) -> bool:
        """Check if this entry is active at given datetime."""
        if dt.weekday() not in self.days:
            return False

        current_time = dt.time()

        if self.start_time <= self.end_time:
            return self.start_time <= current_time <= self.end_time
        else:
            # Handles overnight schedules (e.g., 22:00 to 06:00)
            return current_time >= self.start_time or current_time <= self.end_time


@dataclass
class CircadianSettings:
    """Circadian rhythm adaptation settings."""
    enabled: bool = True

    # Transition times
    sunrise_time: time = time(6, 0)
    sunset_time: time = time(20, 0)

    # Day settings
    day_cct: float = 6500
    day_luminance: float = 120
    day_blue_filter: float = 0

    # Night settings
    night_cct: float = 3400
    night_luminance: float = 80
    night_blue_filter: float = 50

    # Transition duration in minutes
    transition_duration: int = 60


@dataclass
class AdaptationState:
    """Current adaptation system state."""
    mode: AdaptationMode
    active_profile: Optional[DisplayProfile] = None
    current_lux: float = 300
    current_condition: AmbientCondition = AmbientCondition.OFFICE
    last_reading: Optional[AmbientReading] = None
    last_switch: Optional[datetime] = None

    # Smoothed values for stable adaptation
    smoothed_lux: float = 300
    readings_buffer: list[float] = field(default_factory=list)

    # Transition state
    in_transition: bool = False
    transition_progress: float = 0.0
    transition_from: Optional[DisplayProfile] = None
    transition_to: Optional[DisplayProfile] = None


# =============================================================================
# Condition Classification
# =============================================================================

# Lux thresholds for ambient conditions
LUX_THRESHOLDS = {
    AmbientCondition.DARK: (0, 50),
    AmbientCondition.DIM: (50, 200),
    AmbientCondition.OFFICE: (200, 500),
    AmbientCondition.BRIGHT: (500, 1000),
    AmbientCondition.VERY_BRIGHT: (1000, float('inf')),
}


def classify_ambient(lux: float) -> AmbientCondition:
    """Classify ambient condition from lux reading."""
    for condition, (min_lux, max_lux) in LUX_THRESHOLDS.items():
        if min_lux <= lux < max_lux:
            return condition
    return AmbientCondition.OFFICE


def condition_to_string(condition: AmbientCondition) -> str:
    """Convert condition enum to display string."""
    return {
        AmbientCondition.DARK: "Dark (<50 lux)",
        AmbientCondition.DIM: "Dim (50-200 lux)",
        AmbientCondition.OFFICE: "Office (200-500 lux)",
        AmbientCondition.BRIGHT: "Bright (500-1000 lux)",
        AmbientCondition.VERY_BRIGHT: "Very Bright (>1000 lux)",
    }[condition]


# =============================================================================
# Preset Profiles
# =============================================================================

PRESET_PROFILES: dict[str, DisplayProfile] = {
    "day": DisplayProfile(
        name="Day",
        profile_type=ProfileType.DAY,
        whitepoint_cct=6500,
        gamma=2.2,
        luminance=120,
        min_lux=200,
        max_lux=float('inf'),
        blue_light_filter=0,
        priority=10,
    ),
    "night": DisplayProfile(
        name="Night",
        profile_type=ProfileType.NIGHT,
        whitepoint_cct=3400,
        gamma=2.2,
        luminance=80,
        min_lux=0,
        max_lux=100,
        blue_light_filter=50,
        priority=10,
    ),
    "cinema": DisplayProfile(
        name="Cinema",
        profile_type=ProfileType.CINEMA,
        whitepoint_cct=6500,
        gamma=2.4,  # BT.1886
        luminance=100,
        black_level=0.05,
        min_lux=0,
        max_lux=50,
        priority=20,
    ),
    "photo_d50": DisplayProfile(
        name="Photo Editing (D50)",
        profile_type=ProfileType.PHOTO,
        whitepoint_cct=5000,
        gamma=2.2,
        luminance=120,
        priority=5,
    ),
    "photo_d65": DisplayProfile(
        name="Photo Editing (D65)",
        profile_type=ProfileType.PHOTO,
        whitepoint_cct=6500,
        gamma=2.2,
        luminance=120,
        priority=5,
    ),
    "video": DisplayProfile(
        name="Video Editing",
        profile_type=ProfileType.VIDEO,
        whitepoint_cct=6500,
        gamma=2.4,
        luminance=100,
        priority=5,
    ),
    "reading": DisplayProfile(
        name="Reading",
        profile_type=ProfileType.READING,
        whitepoint_cct=4500,
        gamma=2.2,
        luminance=100,
        blue_light_filter=30,
        priority=5,
    ),
}


# =============================================================================
# Ambient Light Sensor Interface
# =============================================================================

class AmbientSensor:
    """
    Abstract ambient light sensor interface.

    Subclass this for specific sensor hardware integration.
    """

    def __init__(self):
        self._last_reading: Optional[AmbientReading] = None
        self._callbacks: list[Callable[[AmbientReading], None]] = []

    def read(self) -> AmbientReading:
        """
        Read current ambient light level.

        Override this method for actual sensor integration.
        """
        # Default implementation returns simulated value
        return AmbientReading(
            timestamp=datetime.now(),
            lux=300,
            cct=6500,
        )

    def start_monitoring(self, interval: float = 5.0) -> None:
        """Start continuous monitoring with given interval in seconds."""
        pass

    def stop_monitoring(self) -> None:
        """Stop continuous monitoring."""
        pass

    def add_callback(self, callback: Callable[[AmbientReading], None]) -> None:
        """Add callback for ambient light changes."""
        self._callbacks.append(callback)

    def remove_callback(self, callback: Callable[[AmbientReading], None]) -> None:
        """Remove callback."""
        if callback in self._callbacks:
            self._callbacks.remove(callback)

    def _notify_callbacks(self, reading: AmbientReading) -> None:
        """Notify all registered callbacks."""
        for callback in self._callbacks:
            try:
                callback(reading)
            except Exception:
                pass


class SimulatedSensor(AmbientSensor):
    """Simulated ambient light sensor for testing."""

    def __init__(self, base_lux: float = 300):
        super().__init__()
        self.base_lux = base_lux
        self._running = False
        self._thread: Optional[threading.Thread] = None

    def read(self) -> AmbientReading:
        """Read simulated ambient light."""
        import random
        # Add some variation
        lux = self.base_lux + random.gauss(0, self.base_lux * 0.1)
        lux = max(1, lux)

        reading = AmbientReading(
            timestamp=datetime.now(),
            lux=lux,
            cct=6500,
        )
        self._last_reading = reading
        return reading

    def set_lux(self, lux: float) -> None:
        """Set simulated lux level."""
        self.base_lux = lux

    def start_monitoring(self, interval: float = 5.0) -> None:
        """Start simulated monitoring."""
        self._running = True

        def monitor_loop():
            while self._running:
                reading = self.read()
                self._notify_callbacks(reading)
                time_module.sleep(interval)

        self._thread = threading.Thread(target=monitor_loop, daemon=True)
        self._thread.start()

    def stop_monitoring(self) -> None:
        """Stop monitoring."""
        self._running = False
        if self._thread:
            self._thread.join(timeout=1.0)


class WindowsLightSensor(AmbientSensor):
    """
    Windows ambient light sensor integration.

    Uses Windows.Devices.Sensors API.
    """

    def __init__(self):
        super().__init__()
        self._sensor = None
        self._available = False
        self._init_sensor()

    def _init_sensor(self) -> None:
        """Initialize Windows light sensor."""
        try:
            # This would use winrt or win32api
            # Placeholder for actual implementation
            pass
        except Exception:
            self._available = False

    @property
    def is_available(self) -> bool:
        return self._available

    def read(self) -> AmbientReading:
        """Read from Windows sensor."""
        if not self._available:
            return AmbientReading(
                timestamp=datetime.now(),
                lux=300,
            )

        # Actual sensor reading would go here
        return AmbientReading(
            timestamp=datetime.now(),
            lux=300,
        )


# =============================================================================
# Adaptation Controller
# =============================================================================

class AdaptationController:
    """
    Controls automatic display adaptation based on ambient conditions.

    Manages profile switching, scheduling, and smooth transitions
    between different display states.
    """

    def __init__(self,
                 mode: AdaptationMode = AdaptationMode.MANUAL,
                 sensor: Optional[AmbientSensor] = None):
        """
        Initialize adaptation controller.

        Args:
            mode: Adaptation mode
            sensor: Ambient light sensor (creates simulated if None)
        """
        self.mode = mode
        self.sensor = sensor or SimulatedSensor()

        # Profiles
        self.profiles: dict[str, DisplayProfile] = dict(PRESET_PROFILES)
        self.active_profile: Optional[DisplayProfile] = None

        # Schedule
        self.schedule: list[ScheduleEntry] = []

        # Circadian settings
        self.circadian = CircadianSettings()

        # State
        self.state = AdaptationState(mode=mode)

        # Smoothing parameters
        self.smoothing_window = 5  # Number of readings to average
        self.hysteresis_lux = 50   # Lux change required to switch

        # Callbacks
        self._profile_changed_callbacks: list[Callable[[DisplayProfile], None]] = []

        # Monitoring
        self._monitoring = False

    # =========================================================================
    # Profile Management
    # =========================================================================

    def add_profile(self, profile: DisplayProfile) -> None:
        """Add or update a display profile."""
        self.profiles[profile.name.lower()] = profile

    def remove_profile(self, name: str) -> None:
        """Remove a profile by name."""
        key = name.lower()
        if key in self.profiles:
            del self.profiles[key]

    def get_profile(self, name: str) -> Optional[DisplayProfile]:
        """Get profile by name."""
        return self.profiles.get(name.lower())

    def set_active_profile(self, profile: DisplayProfile) -> None:
        """Manually set active profile."""
        old_profile = self.active_profile
        self.active_profile = profile
        self.state.active_profile = profile
        self.state.last_switch = datetime.now()

        if old_profile != profile:
            self._notify_profile_changed(profile)

    # =========================================================================
    # Schedule Management
    # =========================================================================

    def add_schedule_entry(self, entry: ScheduleEntry) -> None:
        """Add a schedule entry."""
        self.schedule.append(entry)
        self.schedule.sort(key=lambda e: e.start_time)

    def remove_schedule_entry(self, index: int) -> None:
        """Remove schedule entry by index."""
        if 0 <= index < len(self.schedule):
            del self.schedule[index]

    def get_scheduled_profile(self, dt: Optional[datetime] = None) -> Optional[DisplayProfile]:
        """Get the profile that should be active according to schedule."""
        dt = dt or datetime.now()

        for entry in self.schedule:
            if entry.is_active(dt):
                return self.get_profile(entry.profile_name)

        return None

    # =========================================================================
    # Circadian Adaptation
    # =========================================================================

    def calculate_circadian_settings(self,
                                     dt: Optional[datetime] = None) -> dict[str, float]:
        """
        Calculate display settings based on circadian rhythm.

        Returns interpolated settings between day and night based on time.
        """
        dt = dt or datetime.now()
        current_time = dt.time()

        c = self.circadian

        # Convert times to minutes for easier math
        def time_to_minutes(t: time) -> int:
            return t.hour * 60 + t.minute

        current_mins = time_to_minutes(current_time)
        sunrise_mins = time_to_minutes(c.sunrise_time)
        sunset_mins = time_to_minutes(c.sunset_time)
        transition_mins = c.transition_duration

        # Calculate blend factor (0 = night, 1 = day)
        if sunrise_mins <= current_mins < sunrise_mins + transition_mins:
            # Morning transition
            blend = (current_mins - sunrise_mins) / transition_mins
        elif sunrise_mins + transition_mins <= current_mins < sunset_mins:
            # Daytime
            blend = 1.0
        elif sunset_mins <= current_mins < sunset_mins + transition_mins:
            # Evening transition
            blend = 1.0 - (current_mins - sunset_mins) / transition_mins
        else:
            # Nighttime
            blend = 0.0

        # Interpolate settings
        return {
            "cct": c.night_cct + blend * (c.day_cct - c.night_cct),
            "luminance": c.night_luminance + blend * (c.day_luminance - c.night_luminance),
            "blue_filter": c.night_blue_filter + blend * (c.day_blue_filter - c.night_blue_filter),
            "blend": blend,
        }

    # =========================================================================
    # Ambient Adaptation
    # =========================================================================

    def process_reading(self, reading: AmbientReading) -> Optional[DisplayProfile]:
        """
        Process an ambient light reading and determine if profile switch needed.

        Args:
            reading: Ambient light reading

        Returns:
            New profile if switch needed, None otherwise
        """
        self.state.last_reading = reading
        self.state.current_lux = reading.lux
        self.state.current_condition = reading.condition

        # Update smoothed value
        self.state.readings_buffer.append(reading.lux)
        if len(self.state.readings_buffer) > self.smoothing_window:
            self.state.readings_buffer.pop(0)

        self.state.smoothed_lux = sum(self.state.readings_buffer) / len(self.state.readings_buffer)

        # Check if we should switch profiles
        if self.mode == AdaptationMode.MANUAL:
            return None

        best_profile = self._find_best_profile()

        if best_profile and best_profile != self.active_profile:
            # Apply hysteresis
            if self.active_profile:
                lux_change = abs(self.state.smoothed_lux - self._get_profile_center_lux(self.active_profile))
                if lux_change < self.hysteresis_lux:
                    return None

            self.set_active_profile(best_profile)
            return best_profile

        return None

    def _find_best_profile(self) -> Optional[DisplayProfile]:
        """Find the best profile for current conditions."""
        lux = self.state.smoothed_lux
        now = datetime.now()

        candidates: list[tuple[int, DisplayProfile]] = []

        for profile in self.profiles.values():
            # Check lux range
            if not (profile.min_lux <= lux <= profile.max_lux):
                continue

            # Check time range if applicable
            if profile.is_time_based:
                current_time = now.time()
                if profile.start_time <= profile.end_time:
                    if not (profile.start_time <= current_time <= profile.end_time):
                        continue
                else:
                    if not (current_time >= profile.start_time or current_time <= profile.end_time):
                        continue

            candidates.append((profile.priority, profile))

        if not candidates:
            return None

        # Return highest priority match
        candidates.sort(key=lambda x: x[0], reverse=True)
        return candidates[0][1]

    def _get_profile_center_lux(self, profile: DisplayProfile) -> float:
        """Get center lux value for a profile's range."""
        if profile.max_lux == float('inf'):
            return profile.min_lux + 500
        return (profile.min_lux + profile.max_lux) / 2

    # =========================================================================
    # Monitoring
    # =========================================================================

    def start(self) -> None:
        """Start adaptation monitoring."""
        if self._monitoring:
            return

        self._monitoring = True
        self.sensor.add_callback(self._on_sensor_reading)
        self.sensor.start_monitoring()

    def stop(self) -> None:
        """Stop adaptation monitoring."""
        self._monitoring = False
        self.sensor.stop_monitoring()
        self.sensor.remove_callback(self._on_sensor_reading)

    def _on_sensor_reading(self, reading: AmbientReading) -> None:
        """Handle sensor reading callback."""
        self.process_reading(reading)

    # =========================================================================
    # Callbacks
    # =========================================================================

    def add_profile_changed_callback(self,
                                     callback: Callable[[DisplayProfile], None]) -> None:
        """Add callback for profile changes."""
        self._profile_changed_callbacks.append(callback)

    def remove_profile_changed_callback(self,
                                        callback: Callable[[DisplayProfile], None]) -> None:
        """Remove callback."""
        if callback in self._profile_changed_callbacks:
            self._profile_changed_callbacks.remove(callback)

    def _notify_profile_changed(self, profile: DisplayProfile) -> None:
        """Notify callbacks of profile change."""
        for callback in self._profile_changed_callbacks:
            try:
                callback(profile)
            except Exception:
                pass

    # =========================================================================
    # Configuration Persistence
    # =========================================================================

    def save_config(self, path: str) -> None:
        """Save configuration to JSON file."""
        config = {
            "mode": self.mode.name,
            "profiles": {},
            "schedule": [],
            "circadian": {
                "enabled": self.circadian.enabled,
                "sunrise_time": self.circadian.sunrise_time.isoformat(),
                "sunset_time": self.circadian.sunset_time.isoformat(),
                "day_cct": self.circadian.day_cct,
                "day_luminance": self.circadian.day_luminance,
                "day_blue_filter": self.circadian.day_blue_filter,
                "night_cct": self.circadian.night_cct,
                "night_luminance": self.circadian.night_luminance,
                "night_blue_filter": self.circadian.night_blue_filter,
                "transition_duration": self.circadian.transition_duration,
            },
            "smoothing_window": self.smoothing_window,
            "hysteresis_lux": self.hysteresis_lux,
        }

        # Save custom profiles (not presets)
        for name, profile in self.profiles.items():
            if name not in PRESET_PROFILES:
                config["profiles"][name] = {
                    "name": profile.name,
                    "type": profile.profile_type.name,
                    "whitepoint_cct": profile.whitepoint_cct,
                    "gamma": profile.gamma,
                    "luminance": profile.luminance,
                    "black_level": profile.black_level,
                    "icc_path": profile.icc_path,
                    "lut_path": profile.lut_path,
                    "min_lux": profile.min_lux,
                    "max_lux": profile.max_lux if profile.max_lux != float('inf') else None,
                    "blue_light_filter": profile.blue_light_filter,
                    "priority": profile.priority,
                }

        # Save schedule
        for entry in self.schedule:
            config["schedule"].append({
                "start_time": entry.start_time.isoformat(),
                "end_time": entry.end_time.isoformat(),
                "profile_name": entry.profile_name,
                "days": entry.days,
            })

        with open(path, 'w') as f:
            json.dump(config, f, indent=2)

    def load_config(self, path: str) -> None:
        """Load configuration from JSON file."""
        with open(path, 'r') as f:
            config = json.load(f)

        # Load mode
        self.mode = AdaptationMode[config.get("mode", "MANUAL")]
        self.state.mode = self.mode

        # Load circadian settings
        if "circadian" in config:
            c = config["circadian"]
            self.circadian = CircadianSettings(
                enabled=c.get("enabled", True),
                sunrise_time=time.fromisoformat(c.get("sunrise_time", "06:00")),
                sunset_time=time.fromisoformat(c.get("sunset_time", "20:00")),
                day_cct=c.get("day_cct", 6500),
                day_luminance=c.get("day_luminance", 120),
                day_blue_filter=c.get("day_blue_filter", 0),
                night_cct=c.get("night_cct", 3400),
                night_luminance=c.get("night_luminance", 80),
                night_blue_filter=c.get("night_blue_filter", 50),
                transition_duration=c.get("transition_duration", 60),
            )

        # Load custom profiles
        for name, pdata in config.get("profiles", {}).items():
            profile = DisplayProfile(
                name=pdata["name"],
                profile_type=ProfileType[pdata.get("type", "CUSTOM")],
                whitepoint_cct=pdata.get("whitepoint_cct", 6500),
                gamma=pdata.get("gamma", 2.2),
                luminance=pdata.get("luminance", 120),
                black_level=pdata.get("black_level", 0.5),
                icc_path=pdata.get("icc_path"),
                lut_path=pdata.get("lut_path"),
                min_lux=pdata.get("min_lux", 0),
                max_lux=pdata.get("max_lux") or float('inf'),
                blue_light_filter=pdata.get("blue_light_filter", 0),
                priority=pdata.get("priority", 0),
            )
            self.add_profile(profile)

        # Load schedule
        self.schedule.clear()
        for sdata in config.get("schedule", []):
            entry = ScheduleEntry(
                start_time=time.fromisoformat(sdata["start_time"]),
                end_time=time.fromisoformat(sdata["end_time"]),
                profile_name=sdata["profile_name"],
                days=sdata.get("days", list(range(7))),
            )
            self.add_schedule_entry(entry)

        # Load smoothing parameters
        self.smoothing_window = config.get("smoothing_window", 5)
        self.hysteresis_lux = config.get("hysteresis_lux", 50)


# =============================================================================
# Utility Functions
# =============================================================================

def create_default_schedule() -> list[ScheduleEntry]:
    """Create a default day/night schedule."""
    return [
        ScheduleEntry(
            start_time=time(6, 0),
            end_time=time(20, 0),
            profile_name="day",
        ),
        ScheduleEntry(
            start_time=time(20, 0),
            end_time=time(6, 0),
            profile_name="night",
        ),
    ]


def print_adaptation_status(controller: AdaptationController) -> None:
    """Print current adaptation status."""
    state = controller.state
    print("\n" + "=" * 60)
    print("Ambient Light Adaptation Status")
    print("=" * 60)
    print(f"Mode: {controller.mode.name}")
    print(f"Current Lux: {state.current_lux:.0f}")
    print(f"Smoothed Lux: {state.smoothed_lux:.0f}")
    print(f"Condition: {condition_to_string(state.current_condition)}")
    print()
    if state.active_profile:
        p = state.active_profile
        print(f"Active Profile: {p.name}")
        print(f"  CCT: {p.whitepoint_cct}K")
        print(f"  Gamma: {p.gamma}")
        print(f"  Luminance: {p.luminance} cd/m²")
        print(f"  Blue Filter: {p.blue_light_filter}%")
    else:
        print("Active Profile: None")
    print()
    if controller.circadian.enabled:
        settings = controller.calculate_circadian_settings()
        print(f"Circadian Blend: {settings['blend']:.1%}")
        print(f"  Target CCT: {settings['cct']:.0f}K")
        print(f"  Target Luminance: {settings['luminance']:.0f} cd/m²")
    print("=" * 60)


# =============================================================================
# Module Test
# =============================================================================

if __name__ == "__main__":
    # Test adaptation controller
    controller = AdaptationController(
        mode=AdaptationMode.AUTO_SENSOR,
        sensor=SimulatedSensor(base_lux=300)
    )

    # Add schedule
    for entry in create_default_schedule():
        controller.add_schedule_entry(entry)

    # Simulate readings
    print("Simulating ambient light readings...")

    for lux in [500, 400, 200, 100, 50, 20, 50, 100, 300, 500]:
        reading = AmbientReading(
            timestamp=datetime.now(),
            lux=lux,
        )
        profile = controller.process_reading(reading)
        if profile:
            print(f"Lux {lux} -> Switched to: {profile.name}")
        else:
            print(f"Lux {lux} -> No change (current: {controller.active_profile.name if controller.active_profile else 'None'})")

    print_adaptation_status(controller)

    # Test circadian
    print("\nCircadian settings at different times:")
    for hour in [0, 6, 7, 12, 19, 20, 21]:
        dt = datetime.now().replace(hour=hour, minute=0)
        settings = controller.calculate_circadian_settings(dt)
        print(f"  {hour:02d}:00 - CCT: {settings['cct']:.0f}K, "
              f"Lum: {settings['luminance']:.0f}, "
              f"Blend: {settings['blend']:.1%}")
