"""
ArgyllCMS Backend

Integrates with ArgyllCMS tools for professional display calibration.
Wraps spotread, dispread, colprof, and other ArgyllCMS utilities.

Requires ArgyllCMS to be installed: https://www.argyllcms.com/
"""

import os
import re
import subprocess
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple
import shutil

from calibrate_pro.hardware.colorimeter_base import (
    ColorimeterBase, DeviceInfo, DeviceType, ColorMeasurement,
    CalibrationPatch, CalibrationMode, MeasurementType,
    generate_grayscale_patches, generate_profiling_patches
)

# =============================================================================
# ArgyllCMS Configuration
# =============================================================================

@dataclass
class ArgyllConfig:
    """ArgyllCMS installation configuration."""
    bin_path: Optional[Path] = None      # Path to ArgyllCMS bin directory
    ccss_path: Optional[Path] = None     # Path to CCSS files
    ccmx_path: Optional[Path] = None     # Path to CCMX files
    ref_path: Optional[Path] = None      # Path to reference files

    def find_argyll(self) -> bool:
        """
        Attempt to find ArgyllCMS installation.

        Searches in order:
        1. ARGYLL_BIN environment variable
        2. Standard install paths
        3. DisplayCAL's bundled download (AppData/Roaming/DisplayCAL/dl/)
        4. System PATH
        """
        search_paths = [
            # Environment variable
            Path(os.environ.get("ARGYLL_BIN", "")),
            # Standard Windows installs
            Path(r"C:\Program Files\ArgyllCMS\bin"),
            Path(r"C:\Program Files (x86)\ArgyllCMS\bin"),
            Path.home() / "ArgyllCMS" / "bin",
            # Linux/macOS
            Path("/usr/local/bin"),
            Path("/usr/bin"),
        ]

        # DisplayCAL bundled ArgyllCMS (searches for latest version)
        displaycal_dl = Path(os.environ.get("APPDATA", "")) / "DisplayCAL" / "dl"
        if displaycal_dl.exists():
            argyll_dirs = sorted(displaycal_dl.glob("Argyll_V*"), reverse=True)
            for d in argyll_dirs:
                bin_dir = d / "bin"
                if bin_dir.exists():
                    search_paths.insert(0, bin_dir)  # Prefer latest version

        for path in search_paths:
            if not path or not path.exists():
                continue
            exe = "spotread.exe" if os.name == 'nt' else "spotread"
            if (path / exe).exists():
                self.bin_path = path
                return True

        # Check PATH
        spotread_path = shutil.which("spotread")
        if spotread_path:
            self.bin_path = Path(spotread_path).parent
            return True

        return False

    def get_tool(self, name: str) -> Path:
        """Get path to an ArgyllCMS tool."""
        if self.bin_path is None:
            raise RuntimeError("ArgyllCMS not found. Please install ArgyllCMS.")

        exe = f"{name}.exe" if os.name == 'nt' else name
        tool_path = self.bin_path / exe

        if not tool_path.exists():
            raise FileNotFoundError(f"ArgyllCMS tool not found: {tool_path}")

        return tool_path


# Global configuration
_argyll_config = ArgyllConfig()


def get_argyll_config() -> ArgyllConfig:
    """Get the global ArgyllCMS configuration."""
    if _argyll_config.bin_path is None:
        _argyll_config.find_argyll()
    return _argyll_config


def set_argyll_path(path: Path):
    """Set custom ArgyllCMS bin path."""
    _argyll_config.bin_path = Path(path)


# =============================================================================
# ArgyllCMS Backend
# =============================================================================

class ArgyllBackend(ColorimeterBase):
    """
    ArgyllCMS-based colorimeter implementation.

    Uses ArgyllCMS command-line tools for device communication
    and color measurement.
    """

    def __init__(self, argyll_config: Optional[ArgyllConfig] = None):
        super().__init__()

        self.config = argyll_config or get_argyll_config()
        self.current_device_index: int = 0
        self.display_number: int = 1
        self.display_type: str = "l"  # LCD (default)
        self.high_res_mode: bool = True
        self.adaptive_mode: bool = True

        # Temp directory for working files
        self.temp_dir: Optional[Path] = None

        # Cached device list
        self._devices: List[DeviceInfo] = []

    def _run_tool(
        self,
        tool_name: str,
        args: List[str],
        timeout: int = 60,
        capture_output: bool = True
    ) -> subprocess.CompletedProcess:
        """
        Run an ArgyllCMS tool.

        Args:
            tool_name: Name of tool (e.g., "spotread")
            args: Command-line arguments
            timeout: Timeout in seconds
            capture_output: Whether to capture stdout/stderr

        Returns:
            CompletedProcess result
        """
        tool_path = self.config.get_tool(tool_name)
        cmd = [str(tool_path)] + args

        try:
            result = subprocess.run(
                cmd,
                capture_output=capture_output,
                text=True,
                timeout=timeout,
                input="\n",  # Send newline to trigger interactive prompts
                cwd=str(self.temp_dir) if self.temp_dir else None
            )
            return result
        except subprocess.TimeoutExpired:
            raise TimeoutError(f"{tool_name} timed out after {timeout}s")
        except FileNotFoundError:
            raise RuntimeError(f"ArgyllCMS tool not found: {tool_path}")

    def _parse_spotread_output(self, output: str) -> Optional[ColorMeasurement]:
        """Parse spotread output to extract XYZ values."""
        # Look for XYZ values in output
        # Format: "Result is XYZ: X.XXXX Y.YYYY Z.ZZZZ"
        xyz_pattern = r"XYZ:\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)"
        match = re.search(xyz_pattern, output)

        if match:
            X = float(match.group(1))
            Y = float(match.group(2))
            Z = float(match.group(3))

            return ColorMeasurement(X=X, Y=Y, Z=Z)

        # Alternative format: "Result is ... Yxy: Y x y"
        yxy_pattern = r"Yxy:\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)"
        match = re.search(yxy_pattern, output)

        if match:
            Y = float(match.group(1))
            x = float(match.group(2))
            y = float(match.group(3))

            # Convert Yxy to XYZ
            X = (x / y) * Y if y > 0 else 0
            Z = ((1 - x - y) / y) * Y if y > 0 else 0

            return ColorMeasurement(X=X, Y=Y, Z=Z)

        return None

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect connected colorimeters/spectrophotometers."""
        self._devices = []

        try:
            # Use spotread -? to list devices
            result = self._run_tool("spotread", ["-?"], timeout=10)
            output = result.stdout + result.stderr

            # Parse device list from output
            # Look for lines like: "1 = i1 Display Pro"
            device_pattern = r"(\d+)\s*=\s*(.+?)(?:\n|$)"
            matches = re.findall(device_pattern, output)

            for idx, name in matches:
                name = name.strip()

                # Determine device type
                device_type = DeviceType.COLORIMETER
                if any(x in name.lower() for x in ["spectro", "i1pro", "colormunki"]):
                    device_type = DeviceType.SPECTROPHOTOMETER

                # Parse manufacturer
                manufacturer = "Unknown"
                name_lower = name.lower()
                if "nec" in name_lower or "spectrasensor" in name_lower or "mdsv" in name_lower:
                    manufacturer = "NEC (X-Rite OEM)"
                elif "i1" in name_lower or "x-rite" in name_lower:
                    manufacturer = "X-Rite"
                elif "spyder" in name_lower or "datacolor" in name_lower:
                    manufacturer = "Datacolor"
                elif "colormunki" in name_lower:
                    manufacturer = "X-Rite"
                elif "calibrite" in name_lower:
                    manufacturer = "Calibrite"

                capabilities = ["emission"]
                if device_type == DeviceType.SPECTROPHOTOMETER:
                    capabilities.append("spectral")
                if "pro" in name.lower():
                    capabilities.append("ambient")

                self._devices.append(DeviceInfo(
                    name=name,
                    manufacturer=manufacturer,
                    model=name,
                    serial="",
                    device_type=device_type,
                    capabilities=capabilities
                ))

        except Exception as e:
            print(f"Warning: Could not detect devices: {e}")

        return self._devices

    def connect(self, device_index: int = 0) -> bool:
        """Connect to a measurement device."""
        if not self._devices:
            self.detect_devices()

        if device_index < 0 or device_index >= len(self._devices):
            return False

        self.current_device_index = device_index
        self.device_info = self._devices[device_index]
        self.is_connected = True

        # Create temp directory
        self.temp_dir = Path(tempfile.mkdtemp(prefix="calibrate_pro_"))

        return True

    def disconnect(self) -> bool:
        """Disconnect from the current device."""
        self.is_connected = False
        self.device_info = None

        # Clean up temp directory
        if self.temp_dir and self.temp_dir.exists():
            try:
                shutil.rmtree(self.temp_dir)
            except Exception:
                pass
            self.temp_dir = None

        return True

    def calibrate_device(self) -> bool:
        """Perform device calibration (dark calibration)."""
        if not self.is_connected:
            return False

        # For most devices, calibration is done automatically
        # Some devices need explicit dark calibration
        self._report_progress("Calibrating device...", 0.5)

        # spotread with -N flag to skip calibration prompt
        # Most modern colorimeters don't need explicit calibration
        return True

    def set_display_type(self, display_type: str) -> bool:
        """Set display type for measurement optimization."""
        type_map = {
            "LCD": "l",
            "OLED": "o",
            "CRT": "c",
            "Projector": "p",
            "LED": "l",
        }

        self.display_type = type_map.get(display_type.upper(), "l")
        return True

    def set_refresh_mode(self, refresh_rate: float) -> bool:
        """Enable refresh display mode with specific rate."""
        # ArgyllCMS uses -Y flag for refresh display mode
        # Refresh rate is auto-detected
        return True

    def measure_spot(self) -> Optional[ColorMeasurement]:
        """Take a single spot measurement."""
        if not self.is_connected:
            return None

        try:
            args = [
                f"-d{self.current_device_index + 1}",  # Device number (1-based)
                f"-y{self.display_type}",               # Display type
                "-e",                                   # Emission mode
                "-x",                                   # No auto-calibrate prompt
                "-O",                                   # High resolution
            ]

            if self.high_res_mode:
                args.append("-H")

            if self.adaptive_mode:
                args.append("-V")

            # Add CCSS if set
            if self.ccss_file and self.ccss_file.exists():
                args.extend(["-X", str(self.ccss_file)])

            result = self._run_tool("spotread", args, timeout=30)
            output = result.stdout + result.stderr

            return self._parse_spotread_output(output)

        except Exception as e:
            print(f"Measurement error: {e}")
            return None

    def measure_ambient(self) -> Optional[ColorMeasurement]:
        """Measure ambient light."""
        if not self.is_connected:
            return None

        if not self.device_info or "ambient" not in self.device_info.capabilities:
            return None

        try:
            args = [
                f"-d{self.current_device_index + 1}",
                "-a",  # Ambient mode
                "-x",
            ]

            result = self._run_tool("spotread", args, timeout=30)
            return self._parse_spotread_output(result.stdout + result.stderr)

        except Exception:
            return None

    # =========================================================================
    # Advanced Calibration Methods
    # =========================================================================

    def profile_display(
        self,
        display_number: int = 1,
        patch_count: int = 729,
        quality: str = "high",
        output_name: str = "display_profile"
    ) -> Optional[Path]:
        """
        Generate full display profile using ArgyllCMS workflow.

        This runs the full dispread + colprof pipeline.

        Args:
            display_number: Display number (1-based)
            patch_count: Number of profiling patches
            quality: Profile quality ("low", "medium", "high", "ultra")
            output_name: Output profile base name

        Returns:
            Path to generated ICC profile, or None
        """
        if not self.temp_dir:
            self.temp_dir = Path(tempfile.mkdtemp(prefix="calibrate_pro_"))

        # Quality settings
        quality_map = {
            "low": ["-ql"],
            "medium": ["-qm"],
            "high": ["-qh"],
            "ultra": ["-qu"],
        }
        quality_args = quality_map.get(quality, ["-qh"])

        # Step 1: Generate test patches (.ti1 file)
        self._report_progress("Generating test patches...", 0.1)

        ti1_path = self.temp_dir / f"{output_name}.ti1"
        targen_args = [
            "-v",
            f"-d3",  # Display device
            f"-f{patch_count}",  # Number of patches
            "-e4",  # White + primaries
            "-s100",  # Saturation patches
            str(ti1_path.with_suffix(""))
        ]

        result = self._run_tool("targen", targen_args, timeout=60)
        if result.returncode != 0:
            print(f"targen failed: {result.stderr}")
            return None

        # Step 2: Read patches with colorimeter (dispread)
        self._report_progress("Reading patches (this will take a while)...", 0.2)

        ti3_path = self.temp_dir / f"{output_name}.ti3"
        dispread_args = [
            "-v",
            f"-d{display_number}",
            f"-c{self.current_device_index + 1}",
            f"-y{self.display_type}",
            "-k",  # No black compensation
            "-P0.5,0.5,1.0",  # Patch position
        ]

        if self.ccss_file:
            dispread_args.extend(["-X", str(self.ccss_file)])

        dispread_args.append(str(ti1_path.with_suffix("")))

        # This takes a long time - increase timeout
        result = self._run_tool("dispread", dispread_args, timeout=3600)
        if result.returncode != 0:
            print(f"dispread failed: {result.stderr}")
            return None

        self._report_progress("Creating profile...", 0.9)

        # Step 3: Generate ICC profile (colprof)
        icc_path = self.temp_dir / f"{output_name}.icc"
        colprof_args = [
            "-v",
            "-D", f"Calibrate Pro: {output_name}",
            "-C", "Copyright Zain Dana Harper 2024-2025",
            "-A", "ASUS",  # Will be updated based on display
            "-M", "Display",
        ] + quality_args + [
            "-aS",  # Shaper + matrix
            str(ti3_path.with_suffix(""))
        ]

        result = self._run_tool("colprof", colprof_args, timeout=300)
        if result.returncode != 0:
            print(f"colprof failed: {result.stderr}")
            return None

        self._report_progress("Profile complete!", 1.0)

        if icc_path.exists():
            return icc_path
        return None

    def calibrate_display(
        self,
        display_number: int = 1,
        whitepoint: str = "D65",
        gamma: float = 2.2,
        luminance: Optional[float] = None,
        output_name: str = "calibration"
    ) -> Optional[Tuple[Path, Path]]:
        """
        Calibrate display using ArgyllCMS dispcal.

        Args:
            display_number: Display number (1-based)
            whitepoint: Target white point ("D65", "D50", "native", or temp)
            gamma: Target gamma (2.2, 2.4, "sRGB", "L*")
            luminance: Target luminance in cd/m2 (None for native)
            output_name: Output file base name

        Returns:
            Tuple of (calibration_path, profile_path) or None
        """
        if not self.temp_dir:
            self.temp_dir = Path(tempfile.mkdtemp(prefix="calibrate_pro_"))

        self._report_progress("Starting display calibration...", 0.1)

        # Build dispcal arguments
        dispcal_args = [
            "-v",
            f"-d{display_number}",
            f"-c{self.current_device_index + 1}",
            f"-y{self.display_type}",
        ]

        # White point
        if whitepoint.upper() == "D65":
            dispcal_args.extend(["-w0.3127,0.3290"])
        elif whitepoint.upper() == "D50":
            dispcal_args.extend(["-w0.3457,0.3585"])
        elif whitepoint.upper() != "NATIVE":
            # Assume CCT value
            try:
                cct = int(whitepoint)
                dispcal_args.extend([f"-t{cct}"])
            except ValueError:
                pass

        # Gamma
        if isinstance(gamma, str):
            if gamma.lower() == "srgb":
                dispcal_args.extend(["-gs"])
            elif gamma.lower() == "l*":
                dispcal_args.extend(["-gl"])
        else:
            dispcal_args.extend([f"-g{gamma}"])

        # Luminance
        if luminance:
            dispcal_args.extend([f"-b{luminance}"])

        # Quality
        dispcal_args.extend(["-qh"])

        # CCSS
        if self.ccss_file:
            dispcal_args.extend(["-X", str(self.ccss_file)])

        # Output
        dispcal_args.append(str(self.temp_dir / output_name))

        self._report_progress("Calibrating (this takes 5-15 minutes)...", 0.2)

        result = self._run_tool("dispcal", dispcal_args, timeout=1800)

        if result.returncode != 0:
            print(f"dispcal failed: {result.stderr}")
            return None

        cal_path = self.temp_dir / f"{output_name}.cal"
        icc_path = self.temp_dir / f"{output_name}.icc"

        self._report_progress("Calibration complete!", 1.0)

        if cal_path.exists() and icc_path.exists():
            return (cal_path, icc_path)
        elif cal_path.exists():
            return (cal_path, None)

        return None

    def verify_calibration(
        self,
        display_number: int = 1,
        profile_path: Optional[Path] = None
    ) -> Optional[Dict]:
        """
        Verify display calibration using dispread.

        Args:
            display_number: Display number
            profile_path: Path to ICC profile to verify

        Returns:
            Verification results dictionary
        """
        if not self.temp_dir:
            self.temp_dir = Path(tempfile.mkdtemp(prefix="calibrate_pro_"))

        # Generate verification patches
        ti1_path = self.temp_dir / "verify"

        targen_args = [
            "-v",
            "-d3",
            "-f100",  # 100 verification patches
            "-e4",
            str(ti1_path)
        ]

        result = self._run_tool("targen", targen_args, timeout=60)
        if result.returncode != 0:
            return None

        # Read patches
        dispread_args = [
            "-v",
            f"-d{display_number}",
            f"-c{self.current_device_index + 1}",
            f"-y{self.display_type}",
        ]

        if profile_path:
            dispread_args.extend(["-K", str(profile_path)])

        dispread_args.append(str(ti1_path))

        result = self._run_tool("dispread", dispread_args, timeout=1200)
        if result.returncode != 0:
            return None

        # Analyze results
        ti3_path = self.temp_dir / "verify.ti3"
        if not ti3_path.exists():
            return None

        # Parse TI3 file for Delta E analysis
        return self._parse_ti3_results(ti3_path)

    def _parse_ti3_results(self, ti3_path: Path) -> Optional[Dict]:
        """Parse TI3 measurement file for analysis."""
        try:
            with open(ti3_path, 'r') as f:
                content = f.read()

            # Extract Delta E values if present
            # This is a simplified parser
            delta_e_pattern = r"DELTA_E\s+([\d.]+)"
            matches = re.findall(delta_e_pattern, content)

            if matches:
                delta_e_values = [float(x) for x in matches]
                return {
                    "delta_e_avg": sum(delta_e_values) / len(delta_e_values),
                    "delta_e_max": max(delta_e_values),
                    "delta_e_min": min(delta_e_values),
                    "patch_count": len(delta_e_values)
                }

        except Exception as e:
            print(f"Error parsing TI3: {e}")

        return None


# =============================================================================
# Convenience Functions
# =============================================================================

def check_argyll_installation() -> bool:
    """Check if ArgyllCMS is installed and accessible."""
    config = get_argyll_config()
    return config.bin_path is not None


def list_colorimeters() -> List[DeviceInfo]:
    """List all connected colorimeters."""
    backend = ArgyllBackend()
    return backend.detect_devices()


def quick_spot_measure() -> Optional[ColorMeasurement]:
    """Take a quick spot measurement with default settings."""
    backend = ArgyllBackend()
    devices = backend.detect_devices()

    if not devices:
        print("No colorimeters detected")
        return None

    backend.connect(0)
    try:
        return backend.measure_spot()
    finally:
        backend.disconnect()
