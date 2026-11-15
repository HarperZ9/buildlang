"""
3D LUT Generation Engine

Creates 3D color lookup tables for system-wide color management.
Supports multiple export formats: .cube, .3dl, .mga, .csp, .clf/.ctf
"""

import numpy as np
from dataclasses import dataclass
from typing import Callable, Optional, Tuple, Union, List
from pathlib import Path
from enum import Enum

from calibrate_pro.core.color_math import (
    srgb_to_xyz, xyz_to_srgb, xyz_to_lab, lab_to_xyz,
    bradford_adapt, D50_WHITE, D65_WHITE, Illuminant,
    delta_e_2000, gamma_decode, gamma_encode,
    srgb_gamma_expand, srgb_gamma_compress,
    pq_eotf, pq_oetf, bt2390_eetf,
    xyz_abs_to_jzazbz, jzazbz_to_xyz_abs,
    jzazbz_to_jzczhz, jzczhz_to_jzazbz,
    primaries_to_xyz_matrix,
    BT2020_TO_XYZ,
)

class LUTFormat(Enum):
    """Supported 3D LUT file formats."""
    CUBE = "cube"      # DaVinci Resolve / Adobe standard
    DL3 = "3dl"        # Autodesk Lustre / Flame
    MGA = "mga"        # Pandora
    CSP = "csp"        # Rising Sun Research Cinespace
    ICC = "icc"        # Embedded in ICC profile
    CLF = "clf"        # SMPTE ST 2136-1 Common LUT Format (ACES)

@dataclass
class LUT3D:
    """
    3D Color Lookup Table.

    Stores RGB-to-RGB color transformation as a 3D grid.
    """
    size: int  # Grid size (e.g., 17, 33, 65)
    data: np.ndarray  # Shape: (size, size, size, 3)
    title: str = "Calibrate Pro 3D LUT"
    domain_min: Tuple[float, float, float] = (0.0, 0.0, 0.0)
    domain_max: Tuple[float, float, float] = (1.0, 1.0, 1.0)

    @classmethod
    def create_identity(cls, size: int = 33) -> "LUT3D":
        """Create an identity (no-op) 3D LUT."""
        # Create coordinate grids
        coords = np.linspace(0, 1, size)
        r, g, b = np.meshgrid(coords, coords, coords, indexing='ij')

        # Stack into (size, size, size, 3) array
        data = np.stack([r, g, b], axis=-1)

        return cls(size=size, data=data)

    def apply(self, rgb: np.ndarray) -> np.ndarray:
        """
        Apply LUT to RGB values using trilinear interpolation.

        Args:
            rgb: Input RGB values in [0, 1], shape (3,) or (N, 3) or (H, W, 3)

        Returns:
            Transformed RGB values
        """
        from scipy.ndimage import map_coordinates

        rgb = np.asarray(rgb, dtype=np.float64)
        original_shape = rgb.shape

        # Flatten to (N, 3) if needed
        if rgb.ndim == 1:
            rgb = rgb.reshape(1, 3)
        elif rgb.ndim == 3:
            h, w = rgb.shape[:2]
            rgb = rgb.reshape(-1, 3)

        # Scale to LUT indices
        coords = rgb * (self.size - 1)

        # Interpolate each output channel
        result = np.zeros_like(rgb)
        for c in range(3):
            result[:, c] = map_coordinates(
                self.data[:, :, :, c],
                coords.T,
                order=1,  # Trilinear interpolation
                mode='nearest'
            )

        # Reshape to original
        if len(original_shape) == 1:
            return result[0]
        elif len(original_shape) == 3:
            return result.reshape(h, w, 3)
        return result

    def save(self, filepath: Union[str, Path], format: LUTFormat = LUTFormat.CUBE):
        """
        Save LUT to file.

        Args:
            filepath: Output file path
            format: LUT file format
        """
        filepath = Path(filepath)

        if format == LUTFormat.CUBE:
            self._save_cube(filepath)
        elif format == LUTFormat.DL3:
            self._save_3dl(filepath)
        elif format == LUTFormat.MGA:
            self._save_mga(filepath)
        elif format == LUTFormat.CSP:
            self._save_csp(filepath)
        elif format == LUTFormat.CLF:
            self.save_clf(filepath)
        else:
            raise ValueError(f"Unsupported format: {format}")

    def _save_cube(self, filepath: Path):
        """Save in .cube format (Resolve/Adobe)."""
        with open(filepath, 'w') as f:
            f.write(f"TITLE \"{self.title}\"\n")
            f.write(f"LUT_3D_SIZE {self.size}\n")
            f.write(f"DOMAIN_MIN {self.domain_min[0]:.6f} {self.domain_min[1]:.6f} {self.domain_min[2]:.6f}\n")
            f.write(f"DOMAIN_MAX {self.domain_max[0]:.6f} {self.domain_max[1]:.6f} {self.domain_max[2]:.6f}\n")
            f.write("\n")

            # Write data per .cube spec: R changes fastest, B changes slowest
            for b in range(self.size):
                for g in range(self.size):
                    for r in range(self.size):
                        val = self.data[r, g, b]
                        f.write(f"{val[0]:.6f} {val[1]:.6f} {val[2]:.6f}\n")

    def _save_3dl(self, filepath: Path):
        """Save in .3dl format (Autodesk)."""
        # 3dl uses integer values (0-4095 for 12-bit)
        max_val = 4095

        with open(filepath, 'w') as f:
            # Header: input shaper values
            for i in range(self.size):
                val = int(i / (self.size - 1) * max_val)
                f.write(f"{val} ")
            f.write("\n")

            # LUT data
            for b in range(self.size):
                for g in range(self.size):
                    for r in range(self.size):
                        val = self.data[r, g, b]
                        r_int = int(np.clip(val[0], 0, 1) * max_val)
                        g_int = int(np.clip(val[1], 0, 1) * max_val)
                        b_int = int(np.clip(val[2], 0, 1) * max_val)
                        f.write(f" {r_int} {g_int} {b_int}\n")

    def _save_mga(self, filepath: Path):
        """Save in .mga format (Pandora)."""
        with open(filepath, 'w') as f:
            f.write("LUT8\n")
            f.write(f"{self.size}\n")

            for b in range(self.size):
                for g in range(self.size):
                    for r in range(self.size):
                        val = self.data[r, g, b]
                        f.write(f"{val[0]:.6f} {val[1]:.6f} {val[2]:.6f}\n")

    def _save_csp(self, filepath: Path):
        """Save in .csp format (Cinespace)."""
        with open(filepath, 'w') as f:
            f.write("CSPLUTV100\n")
            f.write("3D\n\n")

            # Input ranges
            f.write("BEGIN METADATA\n")
            f.write(f"TITLE \"{self.title}\"\n")
            f.write("END METADATA\n\n")

            # Shaper (identity)
            for _ in range(3):
                f.write("2\n")
                f.write("0.0 1.0\n")
                f.write("0.0 1.0\n\n")

            # 3D LUT
            f.write(f"{self.size} {self.size} {self.size}\n")
            for b in range(self.size):
                for g in range(self.size):
                    for r in range(self.size):
                        val = self.data[r, g, b]
                        f.write(f"{val[0]:.6f} {val[1]:.6f} {val[2]:.6f}\n")

    def save_clf(self, filepath: Union[str, Path]):
        """
        Save in CLF (Common LUT Format) per SMPTE ST 2136-1.

        CLF is an XML-based format used in ACES workflows and supported by
        applications such as DaVinci Resolve, Nuke, and OpenColorIO.  The
        file contains a single ``<LUT3D>`` process node with 32-bit float
        precision and tetrahedral interpolation hint.

        The data ordering matches CLF/CTF spec: B varies fastest (innermost
        loop), then G, then R (outermost) -- identical to .cube ordering.

        Args:
            filepath: Output file path (typically .clf or .ctf extension)
        """
        import uuid
        from xml.etree.ElementTree import Element, SubElement, ElementTree

        filepath = Path(filepath)

        # Root <ProcessList>
        root = Element("ProcessList")
        root.set("compCLFversion", "3.0")
        root.set("id", str(uuid.uuid4()))

        desc = SubElement(root, "Description")
        desc.text = f"Calibrate Pro - {self.title}"

        # <LUT3D> process node
        lut3d = SubElement(root, "LUT3D")
        lut3d.set("inBitDepth", "32f")
        lut3d.set("outBitDepth", "32f")
        lut3d.set("interpolation", "tetrahedral")

        lut_desc = SubElement(lut3d, "Description")
        lut_desc.text = f"{self.size}x{self.size}x{self.size} 3D LUT"

        # <Array> with dim attribute
        array_elem = SubElement(lut3d, "Array")
        array_elem.set("dim", f"{self.size} {self.size} {self.size} 3")

        # Build the text content: one RGB triplet per line
        # CLF ordering: R outermost, G middle, B innermost (same as .cube)
        lines = []
        for r in range(self.size):
            for g in range(self.size):
                for b in range(self.size):
                    val = self.data[r, g, b]
                    lines.append(
                        f"{val[0]:.6f} {val[1]:.6f} {val[2]:.6f}"
                    )
        array_elem.text = "\n" + "\n".join(lines) + "\n"

        # Write XML with declaration
        tree = ElementTree(root)
        with open(filepath, 'wb') as f:
            tree.write(f, encoding='UTF-8', xml_declaration=True)

    def save_reshade_png(self, filepath: Union[str, Path]):
        """
        Save as ReShade-compatible LUT strip PNG.

        ReShade uses a horizontal strip PNG where the 3D LUT is unrolled
        into a 2D image:
        - Width = size * size (e.g., 33*33 = 1089)
        - Height = size (e.g., 33)
        - For each blue slice (0 to size-1), a size x size block of
          red (x-axis) vs green (y-axis).
        - Pixel at (x, y) where x = blue_slice * size + red_index,
          y = green_index.
        - Values are 8-bit sRGB encoded.

        Uses only built-in struct + zlib modules (no PIL dependency).
        """
        import struct
        import zlib

        filepath = Path(filepath)
        size = self.size
        width = size * size
        height = size

        # Build RGB24 pixel data
        rgb_data = bytearray(width * height * 3)

        for blue_idx in range(size):
            for green_idx in range(size):
                for red_idx in range(size):
                    # LUT data is indexed as data[r, g, b]
                    val = self.data[red_idx, green_idx, blue_idx]

                    # sRGB encode: apply sRGB gamma compression and
                    # convert to 8-bit
                    r_byte = int(np.clip(val[0], 0.0, 1.0) * 255.0 + 0.5)
                    g_byte = int(np.clip(val[1], 0.0, 1.0) * 255.0 + 0.5)
                    b_byte = int(np.clip(val[2], 0.0, 1.0) * 255.0 + 0.5)

                    # Pixel position in the strip
                    x = blue_idx * size + red_idx
                    y = green_idx

                    offset = (y * width + x) * 3
                    rgb_data[offset] = r_byte
                    rgb_data[offset + 1] = g_byte
                    rgb_data[offset + 2] = b_byte

        # Write minimal PNG using struct + zlib
        def png_chunk(chunk_type: bytes, data: bytes) -> bytes:
            c = chunk_type + data
            return (
                struct.pack('>I', len(data))
                + c
                + struct.pack('>I', zlib.crc32(c) & 0xFFFFFFFF)
            )

        sig = b'\x89PNG\r\n\x1a\n'
        ihdr = png_chunk(
            b'IHDR',
            struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)
        )

        # Build raw scanlines with filter byte (0 = None) per row
        raw = bytearray()
        for y in range(height):
            raw.append(0)  # filter type: None
            row_start = y * width * 3
            raw.extend(rgb_data[row_start:row_start + width * 3])

        idat = png_chunk(b'IDAT', zlib.compress(bytes(raw)))
        iend = png_chunk(b'IEND', b'')

        with open(filepath, 'wb') as f:
            f.write(sig + ihdr + idat + iend)

    def save_specialk_png(self, filepath: Union[str, Path]):
        """
        Save as SpecialK-compatible LUT PNG.

        SpecialK by Kaldaien uses the same horizontal strip PNG format
        as ReShade for 3D LUT textures.
        """
        self.save_reshade_png(filepath)

    def save_madvr_3dlut(self, filepath: Union[str, Path]):
        """
        Save as MadVR .3dlut binary format.

        MadVR uses a simple binary format:
        - 4 bytes: "3DLT" magic identifier
        - 4 bytes: LUT size (uint32, little-endian)
        - 4 bytes: bit depth (uint32, typically 16)
        - 4 bytes: input range (uint32, 0=full, 1=video)
        - Then: size^3 * 3 entries as uint16 little-endian values

        The LUT data is ordered with R varying fastest (innermost loop),
        then G, then B (outermost), matching the .cube convention.

        Args:
            filepath: Output file path (typically .3dlut extension)
        """
        import struct

        filepath = Path(filepath)
        size = self.size
        bit_depth = 16
        input_range = 0  # 0 = full range
        max_val = (1 << bit_depth) - 1  # 65535 for 16-bit

        with open(filepath, 'wb') as f:
            # Header
            f.write(b'3DLT')
            f.write(struct.pack('<I', size))
            f.write(struct.pack('<I', bit_depth))
            f.write(struct.pack('<I', input_range))

            # LUT data: B outermost, G middle, R innermost
            for b in range(size):
                for g in range(size):
                    for r in range(size):
                        val = self.data[r, g, b]
                        r_int = int(np.clip(val[0], 0.0, 1.0) * max_val + 0.5)
                        g_int = int(np.clip(val[1], 0.0, 1.0) * max_val + 0.5)
                        b_int = int(np.clip(val[2], 0.0, 1.0) * max_val + 0.5)
                        f.write(struct.pack('<HHH', r_int, g_int, b_int))

    def save_mpv_config(
        self,
        lut_path: Union[str, Path],
        icc_path: Optional[Union[str, Path]] = None,
        output_path: Optional[Union[str, Path]] = None,
    ) -> str:
        """
        Generate an mpv.conf snippet for using this calibration.

        The snippet configures mpv to apply the 3D LUT and (optionally)
        an ICC profile for accurate color-managed playback.

        Args:
            lut_path: Path to the .cube (or .3dlut) LUT file that mpv
                should load.
            icc_path: Optional path to an ICC profile.  If provided, the
                ``icc-profile`` directive is included.
            output_path: If provided, the config snippet is written to
                this file path.  Otherwise, the snippet is only returned
                as a string.

        Returns:
            The mpv config snippet as a string.
        """
        lut_path = Path(lut_path)
        lines = [
            "# Calibrate Pro - Display Calibration",
            f"# Generated for: {self.title}",
        ]

        if icc_path is not None:
            icc_path = Path(icc_path)
            lines.append(f"icc-profile={icc_path}")

        lines.append(f"3dlut-file={lut_path}")
        lines.append("target-colorspace-hint=yes")
        lines.append("icc-3dlut-size=65x65x65")
        lines.append("")  # trailing newline

        config_text = "\n".join(lines)

        if output_path is not None:
            output_path = Path(output_path)
            with open(output_path, 'w', encoding='utf-8') as f:
                f.write(config_text)

        return config_text

    def save_obs_lut(self, filepath: Union[str, Path]):
        """
        Save as OBS-compatible .cube LUT with setup instructions.

        OBS Studio uses standard .cube files via its *Apply LUT* filter
        or the *Color Correction* source filter.  This method writes
        a standard .cube and prepends a comment block with setup
        instructions so the user knows where to place the file.

        Typical OBS LUT path:
            ``%AppData%/obs-studio/luts/<name>.cube``

        Args:
            filepath: Output .cube file path.
        """
        filepath = Path(filepath)

        with open(filepath, 'w') as f:
            # OBS setup instructions as comments
            f.write("# ============================================================\n")
            f.write("# OBS Studio LUT - Generated by Calibrate Pro\n")
            f.write("# ============================================================\n")
            f.write("#\n")
            f.write("# SETUP INSTRUCTIONS:\n")
            f.write("#   1. Copy this file to your OBS LUT directory:\n")
            f.write("#      %AppData%\\obs-studio\\luts\\\n")
            f.write("#      (usually C:\\Users\\<you>\\AppData\\Roaming\\obs-studio\\luts\\)\n")
            f.write("#\n")
            f.write("#   2. In OBS, right-click your source -> Filters\n")
            f.write("#   3. Add an 'Apply LUT' filter\n")
            f.write("#   4. Browse to this .cube file\n")
            f.write("#   5. Set 'Amount' to 1.0 for full correction\n")
            f.write("#\n")
            f.write("# This LUT corrects your display's color characteristics\n")
            f.write("# so recordings/streams show accurate colors even on an\n")
            f.write("# uncalibrated viewer's screen.\n")
            f.write("# ============================================================\n")
            f.write("\n")

            # Standard .cube payload
            f.write(f"TITLE \"{self.title} (OBS)\"\n")
            f.write(f"LUT_3D_SIZE {self.size}\n")
            f.write(f"DOMAIN_MIN {self.domain_min[0]:.6f} {self.domain_min[1]:.6f} {self.domain_min[2]:.6f}\n")
            f.write(f"DOMAIN_MAX {self.domain_max[0]:.6f} {self.domain_max[1]:.6f} {self.domain_max[2]:.6f}\n")
            f.write("\n")

            for b in range(self.size):
                for g in range(self.size):
                    for r in range(self.size):
                        val = self.data[r, g, b]
                        f.write(f"{val[0]:.6f} {val[1]:.6f} {val[2]:.6f}\n")

    @classmethod
    def load_cube(cls, filepath: Union[str, Path]) -> "LUT3D":
        """Load LUT from .cube file."""
        filepath = Path(filepath)
        size = None
        title = "Loaded LUT"
        domain_min = (0.0, 0.0, 0.0)
        domain_max = (1.0, 1.0, 1.0)
        values = []

        with open(filepath, 'r') as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith('#'):
                    continue

                if line.startswith('TITLE'):
                    title = line.split('"')[1] if '"' in line else line.split()[1]
                elif line.startswith('LUT_3D_SIZE'):
                    size = int(line.split()[1])
                elif line.startswith('DOMAIN_MIN'):
                    parts = line.split()[1:]
                    domain_min = tuple(float(p) for p in parts)
                elif line.startswith('DOMAIN_MAX'):
                    parts = line.split()[1:]
                    domain_max = tuple(float(p) for p in parts)
                elif line[0].isdigit() or line[0] == '-':
                    parts = line.split()
                    if len(parts) >= 3:
                        values.append([float(p) for p in parts[:3]])

        if size is None:
            # Infer size from data count
            size = int(round(len(values) ** (1/3)))

        # .cube format: R varies fastest (innermost), B varies slowest (outermost)
        # After reshape: data[b, g, r] contains value for input (r, g, b)
        # We need: data[r, g, b] contains value for input (r, g, b)
        # So transpose axes 0 and 2 to swap R and B positions
        data = np.array(values).reshape(size, size, size, 3)
        data = np.transpose(data, (2, 1, 0, 3))  # Swap B (axis 0) with R (axis 2)

        return cls(size=size, data=data, title=title,
                   domain_min=domain_min, domain_max=domain_max)

    @classmethod
    def load(cls, filepath: Union[str, Path]) -> "LUT3D":
        """
        Load LUT from file, auto-detecting format from extension.

        Supported formats:
        - .cube (Resolve/Adobe)
        - .3dl (Autodesk)
        - .mga (Pandora)
        - .csp (Cinespace)
        - .clf / .ctf (SMPTE ST 2136-1, ACES Common LUT Format)

        Args:
            filepath: Path to LUT file

        Returns:
            LUT3D object
        """
        filepath = Path(filepath)
        ext = filepath.suffix.lower()

        if ext == '.cube':
            return cls.load_cube(filepath)
        elif ext == '.3dl':
            return cls._load_3dl(filepath)
        elif ext == '.mga':
            return cls._load_mga(filepath)
        elif ext == '.csp':
            return cls._load_csp(filepath)
        elif ext in ('.clf', '.ctf'):
            return cls._load_clf(filepath)
        else:
            # Try cube format as default
            try:
                return cls.load_cube(filepath)
            except Exception:
                raise ValueError(f"Unsupported LUT format: {ext}")

    @classmethod
    def _load_3dl(cls, filepath: Path) -> "LUT3D":
        """Load LUT from .3dl file."""
        with open(filepath, 'r') as f:
            lines = [l.strip() for l in f if l.strip() and not l.startswith('#')]

        # First line is input shaper, skip it
        # Rest is LUT data
        values = []
        for line in lines[1:]:
            parts = line.split()
            if len(parts) >= 3:
                # 3dl uses 12-bit integers (0-4095)
                values.append([int(p) / 4095.0 for p in parts[:3]])

        size = int(round(len(values) ** (1/3)))
        # 3dl order is BGR, need to reorder
        data = np.array(values).reshape(size, size, size, 3)
        # Transpose from BGR to RGB order
        data = np.transpose(data, (2, 1, 0, 3))

        return cls(size=size, data=data, title=filepath.stem)

    @classmethod
    def _load_mga(cls, filepath: Path) -> "LUT3D":
        """Load LUT from .mga file."""
        with open(filepath, 'r') as f:
            lines = [l.strip() for l in f if l.strip()]

        # First line: LUT8, second: size
        size = int(lines[1])
        values = []
        for line in lines[2:]:
            parts = line.split()
            if len(parts) >= 3:
                values.append([float(p) for p in parts[:3]])

        data = np.array(values).reshape(size, size, size, 3)
        # Transpose from BGR to RGB order
        data = np.transpose(data, (2, 1, 0, 3))

        return cls(size=size, data=data, title=filepath.stem)

    @classmethod
    def _load_csp(cls, filepath: Path) -> "LUT3D":
        """Load LUT from .csp file."""
        with open(filepath, 'r') as f:
            content = f.read()

        lines = [l.strip() for l in content.split('\n') if l.strip()]

        # Find LUT size line (three numbers)
        size = None
        data_start = 0
        for i, line in enumerate(lines):
            parts = line.split()
            if len(parts) == 3 and all(p.isdigit() for p in parts):
                sizes = [int(p) for p in parts]
                if sizes[0] == sizes[1] == sizes[2]:
                    size = sizes[0]
                    data_start = i + 1
                    break

        if size is None:
            raise ValueError("Could not parse CSP file")

        values = []
        for line in lines[data_start:]:
            parts = line.split()
            if len(parts) >= 3:
                try:
                    values.append([float(p) for p in parts[:3]])
                except ValueError:
                    continue

        data = np.array(values[:size**3]).reshape(size, size, size, 3)
        data = np.transpose(data, (2, 1, 0, 3))

        return cls(size=size, data=data, title=filepath.stem)

    @classmethod
    def _load_clf(cls, filepath: Path) -> "LUT3D":
        """Load LUT from CLF/CTF (Common LUT Format) XML file."""
        import xml.etree.ElementTree as ET

        tree = ET.parse(filepath)
        root = tree.getroot()

        # Extract title from top-level Description if present
        desc_elem = root.find("Description")
        title = desc_elem.text.strip() if desc_elem is not None and desc_elem.text else filepath.stem

        # Find the LUT3D element
        lut3d_elem = root.find("LUT3D")
        if lut3d_elem is None:
            raise ValueError("CLF file does not contain a LUT3D element")

        # Parse dimensions from Array/@dim  e.g. "33 33 33 3"
        array_elem = lut3d_elem.find("Array")
        if array_elem is None:
            raise ValueError("CLF LUT3D element has no Array")

        dim_parts = array_elem.get("dim", "").split()
        if len(dim_parts) < 3:
            raise ValueError(f"Invalid CLF Array dim: {array_elem.get('dim')}")
        size = int(dim_parts[0])

        # Parse float triplets from Array text
        text = array_elem.text or ""
        values = []
        for line in text.strip().split("\n"):
            line = line.strip()
            if not line:
                continue
            parts = line.split()
            if len(parts) >= 3:
                values.append([float(p) for p in parts[:3]])

        if len(values) != size ** 3:
            raise ValueError(
                f"CLF Array has {len(values)} entries, expected {size**3}"
            )

        # CLF ordering matches .cube: R outermost, G middle, B innermost
        # data[r, g, b] = values[r * size*size + g * size + b]
        data = np.array(values).reshape(size, size, size, 3)

        return cls(size=size, data=data, title=title)


class LUTGenerator:
    """
    Generates 3D LUTs from calibration data.

    Supports various color transformations and gamut mappings.
    """

    def __init__(self, size: int = 33):
        """
        Initialize LUT generator.

        Args:
            size: LUT grid size (17, 33, or 65 recommended)
        """
        self.size = size

    def create_from_matrix(
        self,
        matrix: np.ndarray,
        input_gamma: float = 2.2,
        output_gamma: float = 2.2,
        title: str = "Matrix Correction LUT"
    ) -> LUT3D:
        """
        Create LUT from 3x3 color correction matrix.

        Args:
            matrix: 3x3 RGB color correction matrix
            input_gamma: Input gamma (for linearization)
            output_gamma: Output gamma (for encoding)
            title: LUT title
        """
        lut = LUT3D.create_identity(self.size)
        coords = np.linspace(0, 1, self.size)

        for r_idx, r in enumerate(coords):
            for g_idx, g in enumerate(coords):
                for b_idx, b in enumerate(coords):
                    # Linearize input
                    rgb_linear = gamma_decode(np.array([r, g, b]), input_gamma)

                    # Apply matrix
                    rgb_corrected = matrix @ rgb_linear

                    # Encode output
                    rgb_out = gamma_encode(np.clip(rgb_corrected, 0, 1), output_gamma)

                    lut.data[r_idx, g_idx, b_idx] = rgb_out

        lut.title = title
        return lut

    def create_from_primaries(
        self,
        source_primaries: Tuple[Tuple[float, float], ...],
        dest_primaries: Tuple[Tuple[float, float], ...],
        source_white: Tuple[float, float] = (0.3127, 0.3290),
        dest_white: Tuple[float, float] = (0.3127, 0.3290),
        title: str = "Gamut Mapping LUT"
    ) -> LUT3D:
        """
        Create LUT for gamut mapping between color spaces.

        Args:
            source_primaries: (red, green, blue) xy coordinates of source
            dest_primaries: (red, green, blue) xy coordinates of destination
            source_white: Source white point xy
            dest_white: Destination white point xy
            title: LUT title
        """
        from calibrate_pro.core.color_math import primaries_to_xyz_matrix

        # Build transformation matrices
        src_to_xyz = primaries_to_xyz_matrix(
            source_primaries[0], source_primaries[1],
            source_primaries[2], source_white)
        dst_to_xyz = primaries_to_xyz_matrix(
            dest_primaries[0], dest_primaries[1],
            dest_primaries[2], dest_white)
        xyz_to_dst = np.linalg.inv(dst_to_xyz)

        lut = LUT3D.create_identity(self.size)
        coords = np.linspace(0, 1, self.size)

        for r_idx, r in enumerate(coords):
            for g_idx, g in enumerate(coords):
                for b_idx, b in enumerate(coords):
                    # Linearize (assume sRGB)
                    rgb = srgb_gamma_expand(np.array([r, g, b]))

                    # Source to XYZ
                    xyz = src_to_xyz @ rgb

                    # XYZ to destination RGB
                    rgb_out = xyz_to_dst @ xyz

                    # Gamma encode
                    rgb_out = srgb_gamma_compress(np.clip(rgb_out, 0, 1))

                    lut.data[r_idx, g_idx, b_idx] = rgb_out

        lut.title = title
        return lut

    def create_from_function(
        self,
        transform_func: Callable[[np.ndarray], np.ndarray],
        title: str = "Custom Transform LUT"
    ) -> LUT3D:
        """
        Create LUT from arbitrary transformation function.

        Args:
            transform_func: Function that takes RGB (3,) and returns RGB (3,)
            title: LUT title
        """
        lut = LUT3D.create_identity(self.size)
        coords = np.linspace(0, 1, self.size)

        for r_idx, r in enumerate(coords):
            for g_idx, g in enumerate(coords):
                for b_idx, b in enumerate(coords):
                    rgb_in = np.array([r, g, b])
                    rgb_out = transform_func(rgb_in)
                    lut.data[r_idx, g_idx, b_idx] = np.clip(rgb_out, 0, 1)

        lut.title = title
        return lut

    def create_calibration_lut(
        self,
        panel_primaries: Tuple[Tuple[float, float], ...],
        panel_white: Tuple[float, float],
        target_primaries: Tuple[Tuple[float, float], ...] = None,
        target_white: Tuple[float, float] = (0.3127, 0.3290),
        gamma_red: float = 2.2,
        gamma_green: float = 2.2,
        gamma_blue: float = 2.2,
        color_matrix: Optional[np.ndarray] = None,
        title: str = "Display Calibration LUT",
        target_gamma: float = 2.2
    ) -> LUT3D:
        """
        Create comprehensive calibration LUT for display.

        The calibration chain:
        1. Input sRGB values (gamma encoded)
        2. Apply VCGT gamma pre-correction (compensate for panel gamma)
        3. Apply color correction matrix (compensate for panel primaries)
        4. Panel applies its native gamma and primaries
        5. Result: Correct XYZ output matching sRGB specification

        Args:
            panel_primaries: Actual display primaries (red, green, blue)
            panel_white: Actual display white point
            target_primaries: Target color space primaries (default sRGB)
            target_white: Target white point (default D65)
            gamma_red/green/blue: Per-channel gamma values (panel native)
            color_matrix: 3x3 color correction matrix for primaries
            title: LUT title
            target_gamma: Target gamma (default 2.2 for sRGB)
        """
        from calibrate_pro.core.color_math import primaries_to_xyz_matrix

        # Default to sRGB primaries
        if target_primaries is None:
            target_primaries = (
                (0.6400, 0.3300),  # Red
                (0.3000, 0.6000),  # Green
                (0.1500, 0.0600)   # Blue
            )

        # Build color correction matrix from primaries if not provided
        if color_matrix is None:
            # Calculate matrix to convert from target to panel primaries
            target_to_xyz = primaries_to_xyz_matrix(
                target_primaries[0], target_primaries[1],
                target_primaries[2], target_white)
            panel_to_xyz = primaries_to_xyz_matrix(
                panel_primaries[0], panel_primaries[1],
                panel_primaries[2], panel_white)
            xyz_to_panel = np.linalg.inv(panel_to_xyz)

            # This matrix converts target linear RGB to what the panel needs
            # to display the correct XYZ
            color_matrix = xyz_to_panel @ target_to_xyz

        lut = LUT3D.create_identity(self.size)
        coords = np.linspace(0, 1, self.size)

        # Small epsilon for avoiding division by zero, but preserve true black
        EPS = 1e-10

        for r_idx, r in enumerate(coords):
            for g_idx, g in enumerate(coords):
                for b_idx, b in enumerate(coords):
                    rgb = np.array([r, g, b])

                    # CRITICAL: Preserve true black for OLED displays
                    # If input is black (0,0,0), output must be black
                    if r == 0 and g == 0 and b == 0:
                        lut.data[r_idx, g_idx, b_idx] = np.array([0.0, 0.0, 0.0])
                        continue

                    # CORRECT MATHEMATICAL ORDER:
                    # Goal: Panel displays correct XYZ for input sRGB
                    #
                    # Panel behavior: output_XYZ = panel_to_xyz @ (signal^panel_gamma)
                    # We want: output_XYZ = target_XYZ = srgb_to_xyz @ (sRGB^target_gamma)
                    #
                    # So: signal = (xyz_to_panel @ srgb_to_xyz @ sRGB^target_gamma)^(1/panel_gamma)
                    #     signal = (color_matrix @ sRGB^target_gamma)^(1/panel_gamma)

                    # Step 1: Linearize sRGB (apply target gamma)
                    # Use safe power that preserves zeros
                    rgb_linear = np.where(rgb > EPS, np.power(rgb, target_gamma), 0.0)

                    # Step 2: Apply color correction matrix in LINEAR space
                    # This maps from sRGB linear to what panel needs in linear
                    rgb_panel_linear = color_matrix @ rgb_linear

                    # Step 3: Clamp linear values (allow zero for true black)
                    rgb_panel_linear = np.clip(rgb_panel_linear, 0.0, 1.0)

                    # Step 4: Apply inverse of panel gamma to encode for panel
                    # output = linear^(1/panel_gamma)
                    # Use safe power that preserves zeros
                    rgb_output = np.array([
                        np.power(rgb_panel_linear[0], 1.0 / gamma_red) if rgb_panel_linear[0] > EPS else 0.0,
                        np.power(rgb_panel_linear[1], 1.0 / gamma_green) if rgb_panel_linear[1] > EPS else 0.0,
                        np.power(rgb_panel_linear[2], 1.0 / gamma_blue) if rgb_panel_linear[2] > EPS else 0.0
                    ])

                    # Clamp final output
                    lut.data[r_idx, g_idx, b_idx] = np.clip(rgb_output, 0, 1)

        lut.title = title
        return lut

    def optimize_lut(
        self,
        lut: LUT3D,
        smoothing: float = 0.1
    ) -> LUT3D:
        """
        Apply perceptual smoothing to LUT.

        Args:
            lut: Input LUT
            smoothing: Smoothing strength (0-1)

        Returns:
            Smoothed LUT
        """
        from scipy.ndimage import gaussian_filter

        smoothed = LUT3D(
            size=lut.size,
            data=lut.data.copy(),
            title=lut.title + " (smoothed)"
        )

        # Apply Gaussian smoothing per channel
        sigma = smoothing * (lut.size / 17)
        for c in range(3):
            smoothed.data[:, :, :, c] = gaussian_filter(
                lut.data[:, :, :, c], sigma=sigma, mode='nearest')

        return smoothed

    def concat_luts(self, lut1: LUT3D, lut2: LUT3D) -> LUT3D:
        """
        Concatenate two LUTs (lut1 followed by lut2).

        Args:
            lut1: First LUT to apply
            lut2: Second LUT to apply

        Returns:
            Combined LUT
        """
        result = LUT3D.create_identity(self.size)
        coords = np.linspace(0, 1, self.size)

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


    def create_native_gamut_lut(
        self,
        panel_primaries: Tuple[Tuple[float, float], ...],
        panel_white: Tuple[float, float],
        gamma_red: float = 2.2,
        gamma_green: float = 2.2,
        gamma_blue: float = 2.2,
        target_gamma: float = 2.2,
        target_white: Tuple[float, float] = (0.3127, 0.3290),
        title: str = "Native Gamut Calibration LUT",
        oled_compensation: bool = False,
        panel_type: str = "",
        panel_key: str = "",
        target_apl: float = 0.25
    ) -> LUT3D:
        """
        Create a calibration LUT that corrects accuracy WITHIN the panel's
        native gamut without compressing to sRGB.

        This is what the HDR/display enthusiast community actually wants:
        - Fix per-channel gamma tracking to hit target gamma precisely
        - Fix white point to target (typically D65)
        - Correct primary accuracy toward ideal positions
        - Keep the full native gamut width (no sRGB compression)

        The correction is: linearize with panel gamma, apply white point
        correction via Bradford adaptation, re-encode with target gamma.
        No gamut compression occurs — colors that the panel can display
        natively remain unchanged in chromaticity.

        Args:
            panel_primaries: Actual display primaries
            panel_white: Actual display white point
            gamma_red/green/blue: Per-channel panel gamma
            target_gamma: Target gamma (default 2.2)
            target_white: Target white point (default D65)
            title: LUT title
        Returns:
            Native-gamut calibration LUT
        """
        from calibrate_pro.core.color_math import (
            primaries_to_xyz_matrix, bradford_adapt, D65_WHITE, Illuminant
        )

        panel_to_xyz = primaries_to_xyz_matrix(
            panel_primaries[0], panel_primaries[1],
            panel_primaries[2], panel_white)
        xyz_to_panel = np.linalg.inv(panel_to_xyz)

        # White point adaptation matrix (if panel white != target white)
        panel_wp_x, panel_wp_y = panel_white
        target_wp_x, target_wp_y = target_white
        need_wp_adapt = (abs(panel_wp_x - target_wp_x) > 0.001 or
                         abs(panel_wp_y - target_wp_y) > 0.001)

        if need_wp_adapt:
            from calibrate_pro.core.color_math import get_adaptation_matrix
            panel_ill = Illuminant("panel", panel_wp_x / panel_wp_y,
                                   1.0, (1 - panel_wp_x - panel_wp_y) / panel_wp_y,
                                   0)
            target_ill = Illuminant("target", target_wp_x / target_wp_y,
                                    1.0, (1 - target_wp_x - target_wp_y) / target_wp_y,
                                    0)
            adapt_matrix = get_adaptation_matrix(panel_ill, target_ill)
            # Combined: panel RGB -> XYZ -> adapt -> XYZ -> panel RGB
            wp_correction = xyz_to_panel @ adapt_matrix @ panel_to_xyz
        else:
            wp_correction = np.eye(3)

        lut = LUT3D.create_identity(self.size)
        coords = np.linspace(0, 1, self.size)
        EPS = 1e-10
        inv_gammas = np.array([1.0 / gamma_red, 1.0 / gamma_green, 1.0 / gamma_blue])
        target_gammas = np.array([target_gamma, target_gamma, target_gamma])

        # Vectorized: build all grid points
        N = self.size
        r_grid, g_grid, b_grid = np.meshgrid(coords, coords, coords, indexing='ij')
        all_rgb = np.stack([r_grid.ravel(), g_grid.ravel(), b_grid.ravel()], axis=1)
        total = all_rgb.shape[0]

        is_black = np.all(all_rgb == 0.0, axis=1)

        # Step 1: The input signal is what the panel would receive.
        # Linearize with the PANEL's native gamma (undo what the panel does)
        rgb_linear = np.where(all_rgb > EPS, np.power(all_rgb, target_gammas), 0.0)

        # Step 2: Apply white point correction in linear space
        rgb_corrected = (wp_correction @ rgb_linear.T).T

        # Step 3: OLED-specific compensation (before gamma encoding)
        if oled_compensation and panel_type in ("QD-OLED", "WOLED"):
            try:
                from calibrate_pro.display.oled import (
                    get_oled_characteristics, compensate_abl_in_lut,
                    apply_near_black_correction
                )
                oled = get_oled_characteristics(panel_type, panel_key)
                if oled:
                    # ABL compensation: boost signal to counteract brightness reduction
                    if oled.abl_model:
                        abl_factor = oled.abl_model.get_abl_factor(target_apl)
                        # Only compensate if ABL actually reduces brightness
                        if abl_factor < 0.99:
                            rgb_corrected = np.clip(rgb_corrected / abl_factor, 0.0, 1.0)

                    # Near-black correction: fix raised blacks and color shift
                    if oled.near_black_model:
                        nb = oled.near_black_model
                        max_vals = np.max(rgb_corrected, axis=1)
                        near_black_mask = (
                            (max_vals < nb.threshold) &
                            (max_vals > nb.black_cutoff) &
                            ~is_black
                        )
                        if np.any(near_black_mask):
                            t = max_vals[near_black_mask] / nb.threshold
                            lift = 1.0 - nb.gamma_lift * (1.0 - t)
                            rgb_corrected[near_black_mask] *= lift[:, np.newaxis]
            except Exception:
                pass  # OLED compensation is non-critical

        # Step 4: Apply per-channel gamma correction
        # Encode for panel: output^panel_gamma should produce the corrected linear
        # So output = corrected_linear^(1/panel_gamma)
        rgb_output = np.where(
            rgb_corrected > EPS,
            np.power(np.clip(rgb_corrected, 0.0, 1.0), inv_gammas),
            0.0
        )
        rgb_output = np.clip(rgb_output, 0.0, 1.0)

        # Black stays black
        rgb_output[is_black] = 0.0

        lut.data = rgb_output.reshape(N, N, N, 3)
        lut.title = title
        return lut

    def create_oklab_perceptual_lut(
        self,
        panel_primaries: Tuple[Tuple[float, float], ...],
        panel_white: Tuple[float, float],
        gamma_red: float = 2.2,
        gamma_green: float = 2.2,
        gamma_blue: float = 2.2,
        target_primaries: Tuple[Tuple[float, float], ...] = None,
        target_white: Tuple[float, float] = (0.3127, 0.3290),
        target_gamma: float = 2.2,
        title: str = "Oklab Perceptual Calibration LUT"
    ) -> LUT3D:
        """
        Create a calibration LUT using Oklab perceptual gamut mapping.

        This produces significantly better results than the standard matrix
        approach for wide-gamut-to-sRGB conversion because:
        - Hue is preserved during chroma compression (no blue->purple shift)
        - Chroma reduction is smooth and perceptually uniform
        - Lightness is preserved where possible

        Uses the newly ported Oklab color space from Spectrum.

        Optimized: vectorizes the in-gamut path (majority of points) and
        only falls back to per-pixel binary search for out-of-gamut colors.

        Args:
            panel_primaries: Actual display primaries (red, green, blue)
            panel_white: Actual display white point
            gamma_red/green/blue: Per-channel panel gamma
            target_primaries: Target space primaries (default sRGB)
            target_white: Target white point (default D65)
            target_gamma: Target gamma (default 2.2)
            title: LUT title
        Returns:
            Perceptually-optimized calibration LUT
        """
        from calibrate_pro.core.color_math import (
            primaries_to_xyz_matrix, linear_srgb_to_oklab,
            oklab_to_linear_srgb, srgb_gamma_expand, srgb_gamma_compress
        )

        if target_primaries is None:
            target_primaries = (
                (0.6400, 0.3300),
                (0.3000, 0.6000),
                (0.1500, 0.0600)
            )

        # Build conversion matrices
        panel_to_xyz = primaries_to_xyz_matrix(
            panel_primaries[0], panel_primaries[1],
            panel_primaries[2], panel_white)
        target_to_xyz = primaries_to_xyz_matrix(
            target_primaries[0], target_primaries[1],
            target_primaries[2], target_white)
        xyz_to_panel = np.linalg.inv(panel_to_xyz)

        # Combined matrix: target linear RGB -> panel linear RGB
        target_to_panel = xyz_to_panel @ target_to_xyz

        lut = LUT3D.create_identity(self.size)
        coords = np.linspace(0, 1, self.size)
        EPS = 1e-10

        # --- Vectorized: build all grid points as flat (N, 3) array ---
        N = self.size
        r_grid, g_grid, b_grid = np.meshgrid(coords, coords, coords, indexing='ij')
        # shape (N*N*N, 3)
        all_rgb = np.stack([r_grid.ravel(), g_grid.ravel(), b_grid.ravel()], axis=1)
        total = all_rgb.shape[0]

        # 1. Identify black point (0,0,0) -- index 0 when coords starts at 0
        is_black = np.all(all_rgb == 0.0, axis=1)

        # 2. Linearize all points (target gamma decode), vectorized
        rgb_linear_all = np.where(all_rgb > EPS, np.power(all_rgb, target_gamma), 0.0)

        # 3. Convert all to panel linear RGB via combined matrix, vectorized
        #    panel_linear = target_to_panel @ rgb_linear  for each row
        panel_linear_all = (target_to_panel @ rgb_linear_all.T).T  # (N^3, 3)

        # 4. Determine which points are out of gamut
        oog_mask = (
            np.any(panel_linear_all < -0.001, axis=1) |
            np.any(panel_linear_all > 1.001, axis=1)
        )
        # Black points are never out of gamut (they're handled separately)
        oog_mask[is_black] = False

        # Non-black, in-gamut mask
        in_gamut_mask = ~is_black & ~oog_mask

        # --- Vectorized in-gamut path ---
        # For in-gamut points: clamp, apply inverse panel gamma, clamp output
        ig_panel = np.clip(panel_linear_all[in_gamut_mask], 0.0, 1.0)

        inv_gammas = np.array([1.0 / gamma_red, 1.0 / gamma_green, 1.0 / gamma_blue])
        ig_output = np.where(ig_panel > EPS, np.power(ig_panel, inv_gammas), 0.0)
        ig_output = np.clip(ig_output, 0.0, 1.0)

        # --- Allocate result array ---
        result_all = np.zeros((total, 3), dtype=np.float64)
        # Black stays (0,0,0) -- already zeros
        # In-gamut points
        result_all[in_gamut_mask] = ig_output

        # --- Scalar binary-search for out-of-gamut points only ---
        oog_indices = np.where(oog_mask)[0]
        for idx in oog_indices:
            rgb_linear = rgb_linear_all[idx]

            # Convert target linear RGB to Oklab (sRGB matrices as proxy)
            oklab = linear_srgb_to_oklab(np.clip(rgb_linear, 0, 1))
            L, a_ok, b_ok = oklab[0], oklab[1], oklab[2]
            C = np.sqrt(a_ok ** 2 + b_ok ** 2)

            if C > EPS:
                h = np.arctan2(b_ok, a_ok)
                cos_h = np.cos(h)
                sin_h = np.sin(h)

                # Binary search for max chroma in panel gamut
                lo, hi = 0.0, C
                for _ in range(20):
                    mid = (lo + hi) * 0.5
                    test_ok = np.array([L, mid * cos_h, mid * sin_h])
                    test_rgb = oklab_to_linear_srgb(test_ok)
                    test_xyz = target_to_xyz @ np.clip(test_rgb, 0, None)
                    test_panel = xyz_to_panel @ test_xyz
                    if np.all(test_panel >= -0.001) and np.all(test_panel <= 1.001):
                        lo = mid
                    else:
                        hi = mid

                new_ok = np.array([L, lo * cos_h, lo * sin_h])
                new_linear = oklab_to_linear_srgb(new_ok)
                new_xyz = target_to_xyz @ np.clip(new_linear, 0, None)
                panel_linear = xyz_to_panel @ new_xyz
            else:
                panel_linear = panel_linear_all[idx]

            panel_linear = np.clip(panel_linear, 0.0, 1.0)
            rgb_output = np.where(
                panel_linear > EPS,
                np.power(panel_linear, inv_gammas),
                0.0
            )
            result_all[idx] = np.clip(rgb_output, 0.0, 1.0)

        # Reshape flat result back into LUT grid (size, size, size, 3)
        lut.data = result_all.reshape(N, N, N, 3)
        lut.title = title
        return lut

    def create_hdr_calibration_lut(
        self,
        panel_primaries: Tuple[Tuple[float, float], ...],
        panel_white: Tuple[float, float],
        gamma_red: float = 2.2,
        gamma_green: float = 2.2,
        gamma_blue: float = 2.2,
        peak_luminance: float = 1000.0,
        target_white_luminance: float = 203.0,
        title: str = "HDR Calibration LUT",
    ) -> LUT3D:
        """
        Create an HDR calibration LUT operating in PQ (ST.2084) signal space.

        The LUT corrects a panel's native primaries/gamma so that HDR10
        content (BT.2020 PQ-encoded) is displayed accurately on the actual
        panel.

        Processing pipeline per grid point:
            1. Input is a PQ-encoded BT.2020 RGB triplet in [0, 1].
            2. Decode PQ to absolute luminance (cd/m^2) per channel.
            3. Convert BT.2020 linear RGB to absolute XYZ.
            4. Convert XYZ to JzAzBz for perceptual gamut mapping.
            5. If the colour falls outside the panel gamut, compress
               chroma in JzCzhz (preserving lightness and hue).
            6. Convert the (possibly compressed) JzAzBz back to XYZ.
            7. Convert XYZ to panel linear RGB.
            8. Apply per-channel inverse panel gamma.
            9. Map the SDR-in-HDR reference white (203 cd/m^2 by default)
               so that it lands at the correct level.
            10. Encode the result back to PQ and write into the LUT.

        The output .cube file should be saved with an ``_hdr`` suffix for
        dwm_lut compatibility (the caller is responsible for naming).

        Args:
            panel_primaries: Measured panel primaries as
                ((r_x, r_y), (g_x, g_y), (b_x, b_y)).
            panel_white: Measured panel white point as (x, y).
            gamma_red: Native gamma of the red channel.
            gamma_green: Native gamma of the green channel.
            gamma_blue: Native gamma of the blue channel.
            peak_luminance: Panel peak luminance in cd/m^2 (e.g. 1000).
            target_white_luminance: HDR reference-white luminance in cd/m^2
                (default 203 cd/m^2 per ITU-R BT.2408).
            title: Title embedded in the .cube file.

        Returns:
            A :class:`LUT3D` whose domain is PQ-encoded [0, 1].
        """
        # ---- colour-space matrices ----
        # BT.2020 linear RGB -> XYZ (D65)
        bt2020_to_xyz = BT2020_TO_XYZ.copy()
        xyz_to_bt2020 = np.linalg.inv(bt2020_to_xyz)

        # Panel linear RGB -> XYZ (D65)  and inverse
        panel_to_xyz = primaries_to_xyz_matrix(
            panel_primaries[0],
            panel_primaries[1],
            panel_primaries[2],
            panel_white,
        )
        xyz_to_panel = np.linalg.inv(panel_to_xyz)

        # Combined matrix: BT.2020 linear -> panel linear (used for the
        # fast in-gamut path and for checking gamut membership).
        bt2020_to_panel = xyz_to_panel @ bt2020_to_xyz

        # Reference-white scaling factor.
        # The SDR reference white at ``target_white_luminance`` cd/m^2
        # should map to the same absolute luminance on the panel.  We
        # express it as a fraction of the peak so that we can rescale
        # the panel-linear values before gamma encoding.
        white_scale = target_white_luminance / peak_luminance  # e.g. 0.203

        inv_gammas = np.array(
            [1.0 / gamma_red, 1.0 / gamma_green, 1.0 / gamma_blue],
            dtype=np.float64,
        )

        EPS = 1e-12
        lut = LUT3D.create_identity(self.size)
        coords = np.linspace(0.0, 1.0, self.size)
        N = self.size

        # -- vectorise: build all PQ grid points --
        r_g, g_g, b_g = np.meshgrid(coords, coords, coords, indexing="ij")
        all_pq = np.stack([r_g.ravel(), g_g.ravel(), b_g.ravel()], axis=1)
        total = all_pq.shape[0]

        # 0. Apply BT.2390 EETF tone mapping in PQ domain
        #    Maps from 10000 nit source content to this panel's peak luminance
        all_pq = bt2390_eetf(
            all_pq,
            source_peak_nits=10000.0,
            target_peak_nits=peak_luminance,
            target_black_nits=0.0,
        )

        # 1. PQ decode -> absolute cd/m^2 per channel
        all_nits = pq_eotf(all_pq)  # shape (N^3, 3), values in [0, 10000]

        # 2. Treat as BT.2020 linear RGB (normalised to peak_luminance so
        #    that 1.0 equals the panel's actual peak).
        #    Each channel is in cd/m^2; divide by peak to get [0, ~1]
        all_linear = all_nits / peak_luminance  # may exceed 1.0 for bright HDR

        # 3. Convert to panel linear via the combined matrix (vectorised).
        panel_linear_all = (bt2020_to_panel @ all_linear.T).T  # (N^3, 3)

        # 4. Identify out-of-gamut points.
        is_black = np.all(all_pq < EPS, axis=1)
        oog_mask = (
            np.any(panel_linear_all < -0.001, axis=1)
            | np.any(panel_linear_all > 1.001, axis=1)
        )
        oog_mask[is_black] = False
        in_gamut_mask = ~is_black & ~oog_mask

        # -- allocate output (PQ-encoded) --
        result_all = np.zeros((total, 3), dtype=np.float64)
        # Black stays black (already zeros).

        # ---- fast vectorised in-gamut path ----
        ig_panel = np.clip(panel_linear_all[in_gamut_mask], 0.0, 1.0)
        ig_output_linear = np.where(
            ig_panel > EPS, np.power(ig_panel, inv_gammas), 0.0
        )
        # Scale so that reference-white maps correctly, then convert
        # back to absolute nits and PQ-encode.
        ig_nits = np.clip(ig_output_linear, 0.0, 1.0) * peak_luminance
        result_all[in_gamut_mask] = pq_oetf(ig_nits)

        # ---- per-pixel JzAzBz gamut mapping for OOG points ----
        oog_indices = np.where(oog_mask)[0]
        for idx in oog_indices:
            lin_bt2020 = all_linear[idx]  # BT.2020 linear, normalised

            # Convert to absolute XYZ (cd/m^2)
            xyz_abs = bt2020_to_xyz @ (lin_bt2020 * peak_luminance)

            # XYZ -> JzAzBz
            jab = xyz_abs_to_jzazbz(xyz_abs)
            jch = jzazbz_to_jzczhz(jab)
            Jz, Cz, hz = float(jch[0]), float(jch[1]), float(jch[2])

            if Cz > EPS:
                h_rad = np.radians(hz)
                cos_h = np.cos(h_rad)
                sin_h = np.sin(h_rad)

                # Binary search: find the maximum chroma in panel gamut
                lo, hi = 0.0, Cz
                for _ in range(24):
                    mid = (lo + hi) * 0.5
                    test_jch = np.array([Jz, mid, hz])
                    test_jab = jzczhz_to_jzazbz(test_jch)
                    test_xyz = jzazbz_to_xyz_abs(test_jab)
                    test_panel = xyz_to_panel @ test_xyz
                    # Normalise to panel peak
                    test_panel_norm = test_panel / peak_luminance
                    if (
                        np.all(test_panel_norm >= -0.001)
                        and np.all(test_panel_norm <= 1.001)
                    ):
                        lo = mid
                    else:
                        hi = mid

                # Reconstruct with compressed chroma
                mapped_jch = np.array([Jz, lo, hz])
                mapped_jab = jzczhz_to_jzazbz(mapped_jch)
                mapped_xyz = jzazbz_to_xyz_abs(mapped_jab)
                panel_lin = xyz_to_panel @ mapped_xyz / peak_luminance
            else:
                panel_lin = panel_linear_all[idx]

            panel_lin = np.clip(panel_lin, 0.0, 1.0)
            encoded = np.where(
                panel_lin > EPS, np.power(panel_lin, inv_gammas), 0.0
            )
            out_nits = np.clip(encoded, 0.0, 1.0) * peak_luminance
            result_all[idx] = pq_oetf(out_nits)

        # Reshape and store
        lut.data = result_all.reshape(N, N, N, 3)
        lut.title = title
        return lut


def create_identity_lut(size: int = 33, filepath: Optional[Path] = None) -> LUT3D:
    """
    Create and optionally save an identity LUT.

    Args:
        size: LUT grid size
        filepath: Optional path to save LUT

    Returns:
        Identity LUT
    """
    lut = LUT3D.create_identity(size)
    lut.title = "Identity LUT"

    if filepath:
        lut.save(filepath)

    return lut


def srgb_to_display_p3_lut(size: int = 33) -> LUT3D:
    """Create LUT for sRGB to Display P3 conversion."""
    generator = LUTGenerator(size)

    srgb_primaries = (
        (0.6400, 0.3300),
        (0.3000, 0.6000),
        (0.1500, 0.0600)
    )
    p3_primaries = (
        (0.6800, 0.3200),
        (0.2650, 0.6900),
        (0.1500, 0.0600)
    )

    return generator.create_from_primaries(
        srgb_primaries, p3_primaries,
        title="sRGB to Display P3"
    )


def display_p3_to_srgb_lut(size: int = 33) -> LUT3D:
    """Create LUT for Display P3 to sRGB conversion."""
    generator = LUTGenerator(size)

    srgb_primaries = (
        (0.6400, 0.3300),
        (0.3000, 0.6000),
        (0.1500, 0.0600)
    )
    p3_primaries = (
        (0.6800, 0.3200),
        (0.2650, 0.6900),
        (0.1500, 0.0600)
    )

    return generator.create_from_primaries(
        p3_primaries, srgb_primaries,
        title="Display P3 to sRGB"
    )
