"""
LUT Format Handlers

Complete support for reading and writing 3D LUT files in various formats:
- .cube (DaVinci Resolve / Adobe standard)
- .3dl (Autodesk Lustre / Flame)
- .mga (Pandora)
- .cal (ArgyllCMS calibration)
- .csp (Cinespace)
- .spi3d (Sony Imageworks)
- .clf (ACES Common LUT Format - XML)

All formats are normalized to float RGB values in [0, 1] range.
"""

import numpy as np
from dataclasses import dataclass, field
from typing import Optional, Tuple, List, Union, Dict, Any
from pathlib import Path
from enum import Enum
import struct
import re
import xml.etree.ElementTree as ET


class LUTType(Enum):
    """LUT type enumeration."""
    LUT_1D = "1d"
    LUT_3D = "3d"


class LUTFormat(Enum):
    """Supported LUT file formats."""
    CUBE = "cube"       # DaVinci Resolve / Adobe
    DL3 = "3dl"         # Autodesk Lustre / Flame
    MGA = "mga"         # Pandora
    CAL = "cal"         # ArgyllCMS calibration
    CSP = "csp"         # Cinespace
    SPI3D = "spi3d"     # Sony Imageworks
    CLF = "clf"         # ACES Common LUT Format
    ICC = "icc"         # Embedded in ICC profile


@dataclass
class LUT1D:
    """
    1D Lookup Table.

    Used for gamma/transfer function correction.
    """
    size: int
    data: np.ndarray  # Shape: (size, 3) for RGB or (size,) for single channel
    title: str = "1D LUT"
    input_range: Tuple[float, float] = (0.0, 1.0)
    output_range: Tuple[float, float] = (0.0, 1.0)

    @classmethod
    def create_identity(cls, size: int = 1024) -> "LUT1D":
        """Create an identity 1D LUT."""
        data = np.zeros((size, 3), dtype=np.float64)
        values = np.linspace(0, 1, size)
        for c in range(3):
            data[:, c] = values
        return cls(size=size, data=data)

    @classmethod
    def create_gamma(cls, gamma: float, size: int = 1024) -> "LUT1D":
        """Create a gamma correction LUT."""
        lut = cls.create_identity(size)
        lut.data = np.power(lut.data, 1.0 / gamma)
        lut.title = f"Gamma {gamma:.2f}"
        return lut

    def apply(self, values: np.ndarray) -> np.ndarray:
        """Apply 1D LUT to values using linear interpolation."""
        from scipy.interpolate import interp1d

        values = np.asarray(values, dtype=np.float64)
        result = np.zeros_like(values)

        x = np.linspace(self.input_range[0], self.input_range[1], self.size)

        if self.data.ndim == 1:
            interp = interp1d(x, self.data, kind='linear', bounds_error=False, fill_value='extrapolate')
            result = interp(values)
        else:
            for c in range(min(3, values.shape[-1] if values.ndim > 1 else 1)):
                interp = interp1d(x, self.data[:, c], kind='linear', bounds_error=False, fill_value='extrapolate')
                if values.ndim == 1:
                    result = interp(values)
                else:
                    result[..., c] = interp(values[..., c])

        return np.clip(result, self.output_range[0], self.output_range[1])


@dataclass
class LUT3D:
    """
    3D Color Lookup Table.

    Stores RGB-to-RGB color transformation as a 3D grid.
    """
    size: int
    data: np.ndarray  # Shape: (size, size, size, 3)
    title: str = "3D LUT"
    domain_min: Tuple[float, float, float] = (0.0, 0.0, 0.0)
    domain_max: Tuple[float, float, float] = (1.0, 1.0, 1.0)
    comments: List[str] = field(default_factory=list)

    @classmethod
    def create_identity(cls, size: int = 33) -> "LUT3D":
        """Create an identity (no-op) 3D LUT."""
        coords = np.linspace(0, 1, size)
        r, g, b = np.meshgrid(coords, coords, coords, indexing='ij')
        data = np.stack([r, g, b], axis=-1).astype(np.float64)
        return cls(size=size, data=data)

    def apply(self, rgb: np.ndarray) -> np.ndarray:
        """Apply LUT using trilinear interpolation."""
        from scipy.ndimage import map_coordinates

        rgb = np.asarray(rgb, dtype=np.float64)
        original_shape = rgb.shape

        if rgb.ndim == 1:
            rgb = rgb.reshape(1, 3)
        elif rgb.ndim == 3:
            h, w = rgb.shape[:2]
            rgb = rgb.reshape(-1, 3)

        # Scale to LUT indices
        coords = rgb * (self.size - 1)

        result = np.zeros_like(rgb)
        for c in range(3):
            result[:, c] = map_coordinates(
                self.data[:, :, :, c],
                coords.T,
                order=1,
                mode='nearest'
            )

        if len(original_shape) == 1:
            return result[0]
        elif len(original_shape) == 3:
            return result.reshape(h, w, 3)
        return result

    def to_1d_approximation(self, size: int = 256) -> LUT1D:
        """Extract 1D LUT approximation from diagonal."""
        lut1d = LUT1D.create_identity(size)

        for i in range(size):
            t = i / (size - 1)
            idx = t * (self.size - 1)
            idx_floor = int(idx)
            idx_ceil = min(idx_floor + 1, self.size - 1)
            frac = idx - idx_floor

            # Interpolate along diagonal
            val = (1 - frac) * self.data[idx_floor, idx_floor, idx_floor] + \
                  frac * self.data[idx_ceil, idx_ceil, idx_ceil]
            lut1d.data[i] = val

        lut1d.title = f"{self.title} (1D approx)"
        return lut1d


class LUTReader:
    """
    Universal LUT file reader.

    Automatically detects format and returns appropriate LUT object.
    """

    @staticmethod
    def detect_format(filepath: Path) -> LUTFormat:
        """Detect LUT format from file extension and content."""
        suffix = filepath.suffix.lower()

        format_map = {
            '.cube': LUTFormat.CUBE,
            '.3dl': LUTFormat.DL3,
            '.mga': LUTFormat.MGA,
            '.cal': LUTFormat.CAL,
            '.csp': LUTFormat.CSP,
            '.spi3d': LUTFormat.SPI3D,
            '.clf': LUTFormat.CLF,
        }

        return format_map.get(suffix)

    @classmethod
    def read(cls, filepath: Union[str, Path]) -> Union[LUT1D, LUT3D]:
        """
        Read LUT from file.

        Args:
            filepath: Path to LUT file

        Returns:
            LUT1D or LUT3D object
        """
        filepath = Path(filepath)
        if not filepath.exists():
            raise FileNotFoundError(f"LUT file not found: {filepath}")

        fmt = cls.detect_format(filepath)
        if fmt is None:
            raise ValueError(f"Unknown LUT format: {filepath.suffix}")

        readers = {
            LUTFormat.CUBE: cls._read_cube,
            LUTFormat.DL3: cls._read_3dl,
            LUTFormat.MGA: cls._read_mga,
            LUTFormat.CAL: cls._read_cal,
            LUTFormat.CSP: cls._read_csp,
            LUTFormat.SPI3D: cls._read_spi3d,
            LUTFormat.CLF: cls._read_clf,
        }

        return readers[fmt](filepath)

    @staticmethod
    def _read_cube(filepath: Path) -> Union[LUT1D, LUT3D]:
        """Read .cube format (Adobe/Resolve)."""
        size_1d = None
        size_3d = None
        title = "Cube LUT"
        domain_min = (0.0, 0.0, 0.0)
        domain_max = (1.0, 1.0, 1.0)
        values = []
        comments = []

        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                line = line.strip()

                if not line:
                    continue
                if line.startswith('#'):
                    comments.append(line[1:].strip())
                    continue

                upper = line.upper()

                if upper.startswith('TITLE'):
                    # Extract title from quotes or after space
                    if '"' in line:
                        title = line.split('"')[1]
                    else:
                        title = line.split(maxsplit=1)[1] if ' ' in line else ""

                elif upper.startswith('LUT_1D_SIZE'):
                    size_1d = int(line.split()[1])

                elif upper.startswith('LUT_3D_SIZE'):
                    size_3d = int(line.split()[1])

                elif upper.startswith('DOMAIN_MIN'):
                    parts = line.split()[1:]
                    domain_min = tuple(float(p) for p in parts[:3])

                elif upper.startswith('DOMAIN_MAX'):
                    parts = line.split()[1:]
                    domain_max = tuple(float(p) for p in parts[:3])

                elif line[0].lstrip('-').replace('.', '').isdigit():
                    # Data line
                    parts = line.split()
                    if len(parts) >= 3:
                        values.append([float(parts[0]), float(parts[1]), float(parts[2])])
                    elif len(parts) == 1:
                        values.append(float(parts[0]))

        if size_1d:
            data = np.array(values[:size_1d], dtype=np.float64)
            if data.ndim == 1:
                data = np.stack([data, data, data], axis=1)
            return LUT1D(size=size_1d, data=data, title=title)

        elif size_3d:
            data = np.array(values[:size_3d**3]).reshape(size_3d, size_3d, size_3d, 3)
            return LUT3D(
                size=size_3d, data=data, title=title,
                domain_min=domain_min, domain_max=domain_max,
                comments=comments
            )

        else:
            # Infer size from data
            n = len(values)
            size = int(round(n ** (1/3)))
            if size ** 3 == n:
                data = np.array(values).reshape(size, size, size, 3)
                return LUT3D(size=size, data=data, title=title,
                            domain_min=domain_min, domain_max=domain_max)
            else:
                data = np.array(values, dtype=np.float64)
                if data.ndim == 1:
                    data = np.stack([data, data, data], axis=1)
                return LUT1D(size=len(values), data=data, title=title)

    @staticmethod
    def _read_3dl(filepath: Path) -> LUT3D:
        """Read .3dl format (Autodesk Lustre/Flame)."""
        lines = []
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith('#'):
                    lines.append(line)

        if not lines:
            raise ValueError("Empty 3DL file")

        # First line is input shaper (space-separated integers)
        shaper_line = lines[0]
        shaper_values = [int(x) for x in shaper_line.split()]
        size = len(shaper_values)

        # Determine bit depth from max shaper value
        max_val = max(shaper_values) if shaper_values else 4095
        if max_val <= 255:
            scale = 255.0
        elif max_val <= 1023:
            scale = 1023.0
        elif max_val <= 4095:
            scale = 4095.0
        else:
            scale = 65535.0

        # Parse data lines
        values = []
        for line in lines[1:]:
            parts = line.split()
            if len(parts) >= 3:
                r = float(parts[0]) / scale
                g = float(parts[1]) / scale
                b = float(parts[2]) / scale
                values.append([r, g, b])

        # Reshape - 3dl uses B, G, R order (B fastest)
        data = np.array(values, dtype=np.float64)
        data = data.reshape(size, size, size, 3)
        # Reorder from BGR to RGB indexing
        data = np.transpose(data, (2, 1, 0, 3))

        return LUT3D(size=size, data=data, title=filepath.stem)

    @staticmethod
    def _read_mga(filepath: Path) -> LUT3D:
        """Read .mga format (Pandora)."""
        lines = []
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith('#'):
                    lines.append(line)

        if not lines:
            raise ValueError("Empty MGA file")

        # First line should be "LUT8" or similar header
        header = lines[0].upper()

        # Second line is typically the size
        size = int(lines[1])

        # Parse data
        values = []
        for line in lines[2:]:
            parts = line.split()
            if len(parts) >= 3:
                values.append([float(parts[0]), float(parts[1]), float(parts[2])])

        data = np.array(values, dtype=np.float64)
        data = data.reshape(size, size, size, 3)
        # MGA uses BGR order
        data = np.transpose(data, (2, 1, 0, 3))

        return LUT3D(size=size, data=data, title=filepath.stem)

    @staticmethod
    def _read_cal(filepath: Path) -> LUT1D:
        """Read .cal format (ArgyllCMS calibration curves)."""
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            content = f.read()

        # Parse CAL format
        # Format: keyword value pairs and data section
        lines = content.strip().split('\n')

        title = "ArgyllCMS Calibration"
        channels = 3
        size = 256
        data = []

        in_data = False
        for line in lines:
            line = line.strip()

            if not line or line.startswith('#'):
                continue

            if line.upper().startswith('DESCRIPTOR'):
                title = line.split('"')[1] if '"' in line else "CAL LUT"

            elif line.upper().startswith('NUMBER_OF_SETS'):
                channels = int(line.split()[1])

            elif line.upper().startswith('NUMBER_OF_FIELDS'):
                size = int(line.split()[1])

            elif line.upper() == 'BEGIN_DATA':
                in_data = True
                continue

            elif line.upper() == 'END_DATA':
                in_data = False
                continue

            elif in_data:
                parts = line.split()
                if len(parts) >= 3:
                    data.append([float(parts[0]), float(parts[1]), float(parts[2])])
                elif len(parts) == 1:
                    data.append([float(parts[0])] * 3)

        if not data:
            # Try simple space-separated format
            for line in lines:
                parts = line.strip().split()
                try:
                    if len(parts) >= 3:
                        data.append([float(parts[0]), float(parts[1]), float(parts[2])])
                except ValueError:
                    continue

        data = np.array(data, dtype=np.float64)
        return LUT1D(size=len(data), data=data, title=title)

    @staticmethod
    def _read_csp(filepath: Path) -> LUT3D:
        """Read .csp format (Cinespace)."""
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            lines = [line.strip() for line in f if line.strip()]

        idx = 0
        title = "Cinespace LUT"
        size = 0
        values = []

        # Skip header
        while idx < len(lines):
            line = lines[idx]
            if line.upper() == 'CSPLUTV100':
                idx += 1
                continue
            elif line.upper() in ('1D', '3D'):
                lut_type = line.upper()
                idx += 1
                continue
            elif line.upper().startswith('BEGIN METADATA'):
                idx += 1
                while idx < len(lines) and not lines[idx].upper().startswith('END METADATA'):
                    if 'TITLE' in lines[idx].upper():
                        title = lines[idx].split('"')[1] if '"' in lines[idx] else ""
                    idx += 1
                idx += 1
                continue
            elif line[0].isdigit():
                # Shaper section (3 shapers for RGB)
                for _ in range(3):
                    shaper_size = int(lines[idx])
                    idx += 1
                    # Skip shaper lines
                    while idx < len(lines) and lines[idx] and lines[idx][0] != '\n':
                        parts = lines[idx].split()
                        if len(parts) == 2:
                            idx += 1
                        else:
                            break
                break
            idx += 1

        # Parse 3D LUT size
        if idx < len(lines):
            size_parts = lines[idx].split()
            size = int(size_parts[0])
            idx += 1

        # Parse LUT data
        while idx < len(lines):
            parts = lines[idx].split()
            if len(parts) >= 3:
                values.append([float(parts[0]), float(parts[1]), float(parts[2])])
            idx += 1

        data = np.array(values, dtype=np.float64)
        data = data.reshape(size, size, size, 3)
        data = np.transpose(data, (2, 1, 0, 3))

        return LUT3D(size=size, data=data, title=title)

    @staticmethod
    def _read_spi3d(filepath: Path) -> LUT3D:
        """Read .spi3d format (Sony Imageworks)."""
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            lines = [line.strip() for line in f if line.strip() and not line.startswith('#')]

        # First line: "SPILUT 1.0"
        # Second line: "3 3" (dimensions)
        # Third line: size size size
        idx = 0

        # Skip header
        while idx < len(lines) and not lines[idx][0].isdigit():
            idx += 1

        # Parse dimensions
        if idx < len(lines):
            parts = lines[idx].split()
            if len(parts) == 3:
                size = int(parts[0])
                idx += 1

        # Parse data
        values = []
        while idx < len(lines):
            parts = lines[idx].split()
            if len(parts) >= 6:
                # Format: r_in g_in b_in r_out g_out b_out
                values.append([float(parts[3]), float(parts[4]), float(parts[5])])
            idx += 1

        data = np.array(values, dtype=np.float64)
        data = data.reshape(size, size, size, 3)

        return LUT3D(size=size, data=data, title=filepath.stem)

    @staticmethod
    def _read_clf(filepath: Path) -> LUT3D:
        """Read .clf format (ACES Common LUT Format - XML)."""
        tree = ET.parse(filepath)
        root = tree.getroot()

        # Handle namespace
        ns = {'clf': 'urn:AMPAS:CLF:v3.0'}
        if root.tag.startswith('{'):
            ns_uri = root.tag.split('}')[0][1:]
            ns = {'clf': ns_uri}

        title = "CLF LUT"

        # Find ProcessList
        process_list = root.find('.//clf:ProcessList', ns) or root.find('.//ProcessList')

        if process_list is None:
            # Try without namespace
            process_list = root.find('.//ProcessList')

        if process_list is None:
            raise ValueError("No ProcessList found in CLF file")

        # Find LUT3D element
        lut3d_elem = process_list.find('.//clf:LUT3D', ns)
        if lut3d_elem is None:
            lut3d_elem = process_list.find('.//LUT3D')

        if lut3d_elem is None:
            raise ValueError("No LUT3D found in CLF file")

        # Get dimensions
        grid_size = lut3d_elem.get('gridSize', '33 33 33')
        sizes = [int(x) for x in grid_size.split()]
        size = sizes[0]

        # Get array data
        array_elem = lut3d_elem.find('.//clf:Array', ns) or lut3d_elem.find('.//Array')

        if array_elem is not None:
            text = array_elem.text.strip()
            values = [float(x) for x in text.split()]
            data = np.array(values).reshape(size, size, size, 3)
        else:
            data = LUT3D.create_identity(size).data

        return LUT3D(size=size, data=data, title=title)


class LUTWriter:
    """
    Universal LUT file writer.
    """

    @classmethod
    def write(
        cls,
        lut: Union[LUT1D, LUT3D],
        filepath: Union[str, Path],
        fmt: Optional[LUTFormat] = None
    ):
        """
        Write LUT to file.

        Args:
            lut: LUT object to write
            filepath: Output file path
            fmt: Format (auto-detected from extension if None)
        """
        filepath = Path(filepath)

        if fmt is None:
            fmt = LUTReader.detect_format(filepath)
            if fmt is None:
                raise ValueError(f"Cannot determine format for: {filepath}")

        if isinstance(lut, LUT1D):
            writers = {
                LUTFormat.CUBE: cls._write_cube_1d,
                LUTFormat.CAL: cls._write_cal,
            }
        else:
            writers = {
                LUTFormat.CUBE: cls._write_cube_3d,
                LUTFormat.DL3: cls._write_3dl,
                LUTFormat.MGA: cls._write_mga,
                LUTFormat.CSP: cls._write_csp,
                LUTFormat.SPI3D: cls._write_spi3d,
                LUTFormat.CLF: cls._write_clf,
            }

        if fmt not in writers:
            raise ValueError(f"Cannot write {type(lut).__name__} to {fmt.value} format")

        writers[fmt](lut, filepath)

    @staticmethod
    def _write_cube_1d(lut: LUT1D, filepath: Path):
        """Write 1D LUT in .cube format."""
        with open(filepath, 'w') as f:
            f.write(f'# Calibrate Pro 1D LUT\n')
            f.write(f'TITLE "{lut.title}"\n')
            f.write(f'LUT_1D_SIZE {lut.size}\n')
            f.write(f'DOMAIN_MIN {lut.input_range[0]:.6f} {lut.input_range[0]:.6f} {lut.input_range[0]:.6f}\n')
            f.write(f'DOMAIN_MAX {lut.input_range[1]:.6f} {lut.input_range[1]:.6f} {lut.input_range[1]:.6f}\n')
            f.write('\n')

            for i in range(lut.size):
                if lut.data.ndim == 1:
                    val = lut.data[i]
                    f.write(f'{val:.6f} {val:.6f} {val:.6f}\n')
                else:
                    f.write(f'{lut.data[i, 0]:.6f} {lut.data[i, 1]:.6f} {lut.data[i, 2]:.6f}\n')

    @staticmethod
    def _write_cube_3d(lut: LUT3D, filepath: Path):
        """Write 3D LUT in .cube format."""
        with open(filepath, 'w') as f:
            f.write(f'# Calibrate Pro 3D LUT\n')
            for comment in lut.comments:
                f.write(f'# {comment}\n')
            f.write(f'TITLE "{lut.title}"\n')
            f.write(f'LUT_3D_SIZE {lut.size}\n')
            f.write(f'DOMAIN_MIN {lut.domain_min[0]:.6f} {lut.domain_min[1]:.6f} {lut.domain_min[2]:.6f}\n')
            f.write(f'DOMAIN_MAX {lut.domain_max[0]:.6f} {lut.domain_max[1]:.6f} {lut.domain_max[2]:.6f}\n')
            f.write('\n')

            # Write in standard order (B changes fastest, then G, then R)
            for r in range(lut.size):
                for g in range(lut.size):
                    for b in range(lut.size):
                        val = lut.data[r, g, b]
                        f.write(f'{val[0]:.6f} {val[1]:.6f} {val[2]:.6f}\n')

    @staticmethod
    def _write_3dl(lut: LUT3D, filepath: Path):
        """Write 3D LUT in .3dl format (12-bit integers)."""
        max_val = 4095  # 12-bit

        with open(filepath, 'w') as f:
            # Write input shaper
            for i in range(lut.size):
                val = int(i / (lut.size - 1) * max_val)
                f.write(f'{val} ')
            f.write('\n')

            # Write LUT data (BGR order)
            for b in range(lut.size):
                for g in range(lut.size):
                    for r in range(lut.size):
                        val = lut.data[r, g, b]
                        r_int = int(np.clip(val[0], 0, 1) * max_val)
                        g_int = int(np.clip(val[1], 0, 1) * max_val)
                        b_int = int(np.clip(val[2], 0, 1) * max_val)
                        f.write(f' {r_int} {g_int} {b_int}\n')

    @staticmethod
    def _write_mga(lut: LUT3D, filepath: Path):
        """Write 3D LUT in .mga format (Pandora)."""
        with open(filepath, 'w') as f:
            f.write('LUT8\n')
            f.write(f'{lut.size}\n')

            # BGR order
            for b in range(lut.size):
                for g in range(lut.size):
                    for r in range(lut.size):
                        val = lut.data[r, g, b]
                        f.write(f'{val[0]:.6f} {val[1]:.6f} {val[2]:.6f}\n')

    @staticmethod
    def _write_cal(lut: LUT1D, filepath: Path):
        """Write 1D LUT in .cal format (ArgyllCMS)."""
        with open(filepath, 'w') as f:
            f.write('CAL\n\n')
            f.write(f'DESCRIPTOR "{lut.title}"\n')
            f.write('ORIGINATOR "Calibrate Pro"\n')
            f.write('DEVICE_CLASS "DISPLAY"\n')
            f.write('COLOR_REP "RGB"\n\n')
            f.write('NUMBER_OF_FIELDS 4\n')
            f.write('BEGIN_DATA_FORMAT\n')
            f.write('RGB_I RGB_R RGB_G RGB_B\n')
            f.write('END_DATA_FORMAT\n\n')
            f.write(f'NUMBER_OF_SETS {lut.size}\n')
            f.write('BEGIN_DATA\n')

            for i in range(lut.size):
                t = i / (lut.size - 1)
                if lut.data.ndim == 1:
                    r = g = b = lut.data[i]
                else:
                    r, g, b = lut.data[i]
                f.write(f'{t:.6f} {r:.6f} {g:.6f} {b:.6f}\n')

            f.write('END_DATA\n')

    @staticmethod
    def _write_csp(lut: LUT3D, filepath: Path):
        """Write 3D LUT in .csp format (Cinespace)."""
        with open(filepath, 'w') as f:
            f.write('CSPLUTV100\n')
            f.write('3D\n\n')

            f.write('BEGIN METADATA\n')
            f.write(f'TITLE "{lut.title}"\n')
            f.write('END METADATA\n\n')

            # Identity shapers for RGB
            for _ in range(3):
                f.write('2\n')
                f.write('0.0 1.0\n')
                f.write('0.0 1.0\n\n')

            f.write(f'{lut.size} {lut.size} {lut.size}\n')

            for b in range(lut.size):
                for g in range(lut.size):
                    for r in range(lut.size):
                        val = lut.data[r, g, b]
                        f.write(f'{val[0]:.6f} {val[1]:.6f} {val[2]:.6f}\n')

    @staticmethod
    def _write_spi3d(lut: LUT3D, filepath: Path):
        """Write 3D LUT in .spi3d format (Sony Imageworks)."""
        with open(filepath, 'w') as f:
            f.write('SPILUT 1.0\n')
            f.write('3 3\n')
            f.write(f'{lut.size} {lut.size} {lut.size}\n')

            for r in range(lut.size):
                for g in range(lut.size):
                    for b in range(lut.size):
                        r_in = r / (lut.size - 1)
                        g_in = g / (lut.size - 1)
                        b_in = b / (lut.size - 1)
                        val = lut.data[r, g, b]
                        f.write(f'{r_in:.6f} {g_in:.6f} {b_in:.6f} {val[0]:.6f} {val[1]:.6f} {val[2]:.6f}\n')

    @staticmethod
    def _write_clf(lut: LUT3D, filepath: Path):
        """Write 3D LUT in .clf format (ACES Common LUT Format)."""
        ns = 'urn:AMPAS:CLF:v3.0'

        root = ET.Element('ProcessList', {
            'xmlns': ns,
            'id': lut.title.replace(' ', '_'),
            'compCLFversion': '3.0'
        })

        # Description
        desc = ET.SubElement(root, 'Description')
        desc.text = lut.title

        # LUT3D element
        lut3d = ET.SubElement(root, 'LUT3D', {
            'id': 'lut3d',
            'interpolation': 'trilinear',
            'gridSize': f'{lut.size} {lut.size} {lut.size}'
        })

        # Array data
        array = ET.SubElement(lut3d, 'Array', {
            'dim': f'{lut.size} {lut.size} {lut.size} 3'
        })

        values = []
        for r in range(lut.size):
            for g in range(lut.size):
                for b in range(lut.size):
                    val = lut.data[r, g, b]
                    values.extend([f'{val[0]:.6f}', f'{val[1]:.6f}', f'{val[2]:.6f}'])

        array.text = '\n' + ' '.join(values) + '\n'

        tree = ET.ElementTree(root)
        ET.indent(tree, space='  ')
        tree.write(filepath, encoding='utf-8', xml_declaration=True)


# Convenience functions

def load_lut(filepath: Union[str, Path]) -> Union[LUT1D, LUT3D]:
    """Load a LUT file (auto-detect format)."""
    return LUTReader.read(filepath)


def save_lut(
    lut: Union[LUT1D, LUT3D],
    filepath: Union[str, Path],
    fmt: Optional[LUTFormat] = None
):
    """Save a LUT to file."""
    LUTWriter.write(lut, filepath, fmt)


def convert_lut(
    input_path: Union[str, Path],
    output_path: Union[str, Path],
    output_format: Optional[LUTFormat] = None
):
    """Convert LUT between formats."""
    lut = load_lut(input_path)
    save_lut(lut, output_path, output_format)


def create_identity_lut(size: int = 33, type: LUTType = LUTType.LUT_3D) -> Union[LUT1D, LUT3D]:
    """Create an identity LUT."""
    if type == LUTType.LUT_1D:
        return LUT1D.create_identity(size)
    return LUT3D.create_identity(size)


def combine_luts(lut1: LUT3D, lut2: LUT3D, size: int = 33) -> LUT3D:
    """Combine two 3D LUTs (lut1 followed by lut2)."""
    result = LUT3D.create_identity(size)
    coords = np.linspace(0, 1, size)

    for r_idx, r in enumerate(coords):
        for g_idx, g in enumerate(coords):
            for b_idx, b in enumerate(coords):
                # Apply first LUT
                rgb = lut1.apply(np.array([r, g, b]))
                # Apply second LUT
                rgb = lut2.apply(rgb)
                result.data[r_idx, g_idx, b_idx] = rgb

    result.title = f"{lut1.title} + {lut2.title}"
    return result


def resize_lut(lut: LUT3D, new_size: int) -> LUT3D:
    """Resize a 3D LUT to a different grid size."""
    result = LUT3D.create_identity(new_size)
    coords = np.linspace(0, 1, new_size)

    for r_idx, r in enumerate(coords):
        for g_idx, g in enumerate(coords):
            for b_idx, b in enumerate(coords):
                result.data[r_idx, g_idx, b_idx] = lut.apply(np.array([r, g, b]))

    result.title = lut.title
    result.domain_min = lut.domain_min
    result.domain_max = lut.domain_max
    return result


def invert_lut(lut: LUT3D, size: int = 33, iterations: int = 10) -> LUT3D:
    """
    Approximate inverse of a 3D LUT using iterative refinement.

    Note: Perfect inversion is not always possible for non-bijective LUTs.
    """
    inverse = LUT3D.create_identity(size)
    coords = np.linspace(0, 1, size)

    for r_idx, r in enumerate(coords):
        for g_idx, g in enumerate(coords):
            for b_idx, b in enumerate(coords):
                target = np.array([r, g, b])
                estimate = target.copy()

                # Newton-Raphson style iteration
                for _ in range(iterations):
                    current = lut.apply(estimate)
                    error = target - current
                    estimate = np.clip(estimate + error * 0.5, 0, 1)

                inverse.data[r_idx, g_idx, b_idx] = estimate

    inverse.title = f"{lut.title} (inverse)"
    return inverse
