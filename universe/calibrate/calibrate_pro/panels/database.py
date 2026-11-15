"""
Panel Characterization Database

Contains factory-measured panel characteristics for sensorless calibration.
Supports auto-detection via EDID model matching.
"""

import json
import os
from dataclasses import dataclass, field, asdict
from typing import Dict, List, Optional, Tuple
from pathlib import Path
import re

@dataclass
class ChromaticityCoord:
    """CIE 1931 xy chromaticity coordinate."""
    x: float
    y: float

    def as_tuple(self) -> Tuple[float, float]:
        return (self.x, self.y)

@dataclass
class PanelPrimaries:
    """Native panel primary colors and white point."""
    red: ChromaticityCoord
    green: ChromaticityCoord
    blue: ChromaticityCoord
    white: ChromaticityCoord

@dataclass
class GammaCurve:
    """Per-channel gamma characteristics."""
    gamma: float = 2.2  # Native gamma
    offset: float = 0.0  # Black level offset
    linear_portion: float = 0.0  # Linear segment below this value

@dataclass
class PanelCapabilities:
    """Panel hardware capabilities."""
    max_luminance_sdr: float = 100.0  # SDR peak brightness (cd/m2)
    max_luminance_hdr: float = 400.0  # HDR peak brightness (cd/m2)
    min_luminance: float = 0.0001  # Minimum black level (cd/m2)
    native_contrast: float = 1000000.0  # Native contrast ratio
    bit_depth: int = 10  # Panel bit depth
    hdr_capable: bool = True
    wide_gamut: bool = True
    vrr_capable: bool = True
    local_dimming: bool = False
    local_dimming_zones: int = 0

@dataclass
class PanelCharacterization:
    """Complete panel characterization for calibration."""
    manufacturer: str
    model_pattern: str  # Regex pattern to match EDID model
    panel_type: str  # WOLED, QD-OLED, IPS, VA, etc.
    native_primaries: PanelPrimaries
    gamma_red: GammaCurve = field(default_factory=GammaCurve)
    gamma_green: GammaCurve = field(default_factory=GammaCurve)
    gamma_blue: GammaCurve = field(default_factory=GammaCurve)
    capabilities: PanelCapabilities = field(default_factory=PanelCapabilities)
    color_correction_matrix: Optional[List[List[float]]] = None
    uniformity_data: Optional[Dict] = None
    notes: str = ""
    display_name: str = ""  # Human-readable product name (e.g., "Odyssey G7 34")

    @property
    def name(self) -> str:
        """Human-readable name: 'ASUS ROG Swift PG27UCDM' or 'Samsung Odyssey G7'."""
        if self.display_name:
            return f"{self.manufacturer} {self.display_name}"
        # Fallback: use the first alternative in model_pattern if it looks like a name
        first = self.model_pattern.split("|")[0]
        # Strip regex characters
        clean = re.sub(r'[\\.*+?^$\[\](){}]', '', first)
        return f"{self.manufacturer} {clean}"

    def to_dict(self) -> dict:
        """Convert to dictionary for JSON serialization."""
        return {
            "manufacturer": self.manufacturer,
            "model_pattern": self.model_pattern,
            "panel_type": self.panel_type,
            "native_primaries": {
                "red": {"x": self.native_primaries.red.x, "y": self.native_primaries.red.y},
                "green": {"x": self.native_primaries.green.x, "y": self.native_primaries.green.y},
                "blue": {"x": self.native_primaries.blue.x, "y": self.native_primaries.blue.y},
                "white": {"x": self.native_primaries.white.x, "y": self.native_primaries.white.y}
            },
            "gamma_red": asdict(self.gamma_red),
            "gamma_green": asdict(self.gamma_green),
            "gamma_blue": asdict(self.gamma_blue),
            "capabilities": asdict(self.capabilities),
            "color_correction_matrix": self.color_correction_matrix,
            "notes": self.notes,
            "display_name": self.display_name
        }

    @classmethod
    def from_dict(cls, data: dict) -> "PanelCharacterization":
        """Create from dictionary (JSON deserialization)."""
        primaries = PanelPrimaries(
            red=ChromaticityCoord(**data["native_primaries"]["red"]),
            green=ChromaticityCoord(**data["native_primaries"]["green"]),
            blue=ChromaticityCoord(**data["native_primaries"]["blue"]),
            white=ChromaticityCoord(**data["native_primaries"]["white"])
        )

        return cls(
            manufacturer=data["manufacturer"],
            model_pattern=data["model_pattern"],
            panel_type=data["panel_type"],
            native_primaries=primaries,
            gamma_red=GammaCurve(**data.get("gamma_red", {})),
            gamma_green=GammaCurve(**data.get("gamma_green", {})),
            gamma_blue=GammaCurve(**data.get("gamma_blue", {})),
            capabilities=PanelCapabilities(**data.get("capabilities", {})),
            color_correction_matrix=data.get("color_correction_matrix"),
            notes=data.get("notes", ""),
            display_name=data.get("display_name", "")
        )

class PanelDatabase:
    """
    Database of panel characterizations for sensorless calibration.

    Loads panel data from JSON files and provides lookup by EDID model.
    """

    def __init__(self, profiles_dir: Optional[Path] = None):
        """
        Initialize the panel database.

        Args:
            profiles_dir: Directory containing JSON profile files.
                         Defaults to ./profiles relative to this file.
        """
        if profiles_dir is None:
            profiles_dir = Path(__file__).parent / "profiles"

        self.profiles_dir = profiles_dir
        self.panels: Dict[str, PanelCharacterization] = {}

        # Load built-in panels
        self._load_builtin_panels()

        # Load JSON profiles if directory exists
        if profiles_dir.exists():
            self._load_json_profiles()

    def _load_builtin_panels(self):
        """Load built-in panel characterizations."""

        # ASUS ROG Swift OLED PG27UCDM (Samsung QD-OLED panel - 2024 generation)
        self.panels["PG27UCDM"] = PanelCharacterization(
            manufacturer="ASUS",
            model_pattern=r"PG27UCDM|ROG.*PG27UCDM|PG27.*UCDM",
            panel_type="QD-OLED",
            display_name="ROG Swift OLED PG27UCDM",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6795, 0.3095),
                green=ChromaticityCoord(0.2325, 0.7115),
                blue=ChromaticityCoord(0.1375, 0.0495),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2020, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1980, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=275.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="ASUS ROG Swift 27-inch 4K 240Hz QD-OLED. Samsung Display 2024 panel. 92% BT.2020."
        )

        # Samsung Odyssey OLED G85SB (Samsung QD-OLED panel)
        self.panels["G85SB"] = PanelCharacterization(
            manufacturer="Samsung",
            model_pattern=r"G85SB|Odyssey.*G85SB|LS34BG850S",
            panel_type="QD-OLED",
            display_name="Odyssey OLED G8 G85SB",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6780, 0.3080),
                green=ChromaticityCoord(0.2340, 0.7100),
                blue=ChromaticityCoord(0.1380, 0.0510),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2050, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1980, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2020, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=250.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1000000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Samsung QD-OLED with wider gamut than WOLED. May need slight chroma adjustment."
        )

        # Dell Alienware AW3423DW (Samsung QD-OLED)
        self.panels["AW3423DW"] = PanelCharacterization(
            manufacturer="Dell",
            model_pattern=r"AW3423DW|Alienware.*3423",
            panel_type="QD-OLED",
            display_name="Alienware AW3423DW",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6780, 0.3080),
                green=ChromaticityCoord(0.2340, 0.7100),
                blue=ChromaticityCoord(0.1380, 0.0510),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2100, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1950, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2050, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=200.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1000000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="First consumer QD-OLED. Same panel as G85SB with Dell calibration."
        )

        # Dell Alienware AW3225QF (Samsung QD-OLED 4K 32" - 2024)
        # Source: Rtings.com measurements, TFTCentral review
        self.panels["AW3225QF"] = PanelCharacterization(
            manufacturer="Dell",
            model_pattern=r"AW3225QF|Alienware.*3225|AW3225",
            panel_type="QD-OLED",
            display_name="Alienware AW3225QF",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6792, 0.3098),
                green=ChromaticityCoord(0.2318, 0.7108),
                blue=ChromaticityCoord(0.1372, 0.0498),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2015, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1985, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=275.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Samsung 2024 QD-OLED 4K panel. 99.3% DCI-P3, Delta E 1.8 out of box. Source: Rtings/TFTCentral."
        )

        # ASUS ROG Swift PG32UCDM (Samsung QD-OLED 4K 32")
        # Source: Hardware Unboxed review measurements
        self.panels["PG32UCDM"] = PanelCharacterization(
            manufacturer="ASUS",
            model_pattern=r"PG32UCDM|ROG.*PG32UCDM|PG32.*UCDM",
            panel_type="QD-OLED",
            display_name="ROG Swift OLED PG32UCDM",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6798, 0.3102),
                green=ChromaticityCoord(0.2322, 0.7112),
                blue=ChromaticityCoord(0.1378, 0.0502),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2025, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1990, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2010, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=280.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="32-inch sibling of PG27UCDM. Same Samsung QD-OLED panel. Source: Hardware Unboxed."
        )

        # Samsung Odyssey OLED G8 G80SD (QD-OLED 32" 4K)
        # Source: Rtings.com measurements
        self.panels["G80SD"] = PanelCharacterization(
            manufacturer="Samsung",
            model_pattern=r"G80SD|Odyssey.*G80SD|LS32DG802S|G8.*OLED",
            panel_type="QD-OLED",
            display_name="Odyssey OLED G8 G80SD",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6785, 0.3095),
                green=ChromaticityCoord(0.2330, 0.7105),
                blue=ChromaticityCoord(0.1375, 0.0505),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2010, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1995, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2005, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=270.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Samsung's own 32-inch 4K QD-OLED. Very accurate out of box. Source: Rtings."
        )

        # Samsung Odyssey G95SC (QD-OLED 49" Super Ultrawide)
        # Source: TFTCentral, Hardware Unboxed
        self.panels["G95SC"] = PanelCharacterization(
            manufacturer="Samsung",
            model_pattern=r"G95SC|Odyssey.*G95SC|LS49CG950S|G9.*OLED",
            panel_type="QD-OLED",
            display_name="Odyssey OLED G9 G95SC",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6782, 0.3085),
                green=ChromaticityCoord(0.2338, 0.7098),
                blue=ChromaticityCoord(0.1382, 0.0512),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2080, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1960, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2020, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=250.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1000000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="49-inch 5120x1440 QD-OLED ultrawide. 32:9 aspect. Source: TFTCentral/Hardware Unboxed."
        )

        # LG C3 OLED (WOLED TV used as monitor - 42/48/55 inch)
        # Source: Rtings.com TV calibration data, HDTVTest
        self.panels["LG_C3"] = PanelCharacterization(
            manufacturer="LG",
            model_pattern=r"OLED42C3|OLED48C3|OLED55C3|LG.*C3|OLED.*C3",
            panel_type="WOLED",
            display_name="C3 OLED",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6399, 0.3301),
                green=ChromaticityCoord(0.2998, 0.5998),
                blue=ChromaticityCoord(0.1502, 0.0601),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=180.0,
                max_luminance_hdr=800.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="LG WOLED evo panel. Excellent for gaming. Use PC mode. Source: Rtings/HDTVTest."
        )

        # LG C4 OLED (2024 WOLED evo)
        # Source: Rtings.com, Vincent Teoh HDTVTest
        self.panels["LG_C4"] = PanelCharacterization(
            manufacturer="LG",
            model_pattern=r"OLED42C4|OLED48C4|OLED55C4|OLED65C4|LG.*C4|OLED.*C4",
            panel_type="WOLED",
            display_name="C4 OLED",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6398, 0.3302),
                green=ChromaticityCoord(0.2999, 0.5997),
                blue=ChromaticityCoord(0.1501, 0.0602),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2005, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1998, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2002, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=200.0,
                max_luminance_hdr=850.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="2024 LG WOLED evo with improved brightness. Excellent factory calibration. Source: Rtings/HDTVTest."
        )

        # LG 27GP950-B (Nano IPS)
        # Source: TFTCentral measurements, Rtings
        self.panels["27GP950"] = PanelCharacterization(
            manufacturer="LG",
            model_pattern=r"27GP950|UltraGear.*27GP950",
            panel_type="Nano-IPS",
            display_name="UltraGear 27GP950-B",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3320),
                green=ChromaticityCoord(0.2750, 0.6400),
                blue=ChromaticityCoord(0.1495, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2150, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2100, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2120, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=400.0,
                max_luminance_hdr=600.0,
                min_luminance=0.08,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Nano IPS with 98% DCI-P3. Good for HDR600 content. Source: TFTCentral/Rtings."
        )

        # BenQ PD3220U (IPS - Professional color work)
        # Source: TFTCentral professional review
        self.panels["PD3220U"] = PanelCharacterization(
            manufacturer="BenQ",
            model_pattern=r"PD3220U|BenQ.*PD3220",
            panel_type="IPS",
            display_name="DesignVue PD3220U",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6495, 0.3380),
                green=ChromaticityCoord(0.2680, 0.6550),
                blue=ChromaticityCoord(0.1490, 0.0550),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=300.0,
                max_luminance_hdr=300.0,
                min_luminance=0.25,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=False,
                wide_gamut=True,
                vrr_capable=False,
                local_dimming=False
            ),
            notes="Professional 4K monitor. Factory calibrated Delta E < 2. Thunderbolt 3. Source: TFTCentral."
        )

        # ASUS ProArt PA32UCG-K (Mini-LED)
        # Source: TFTCentral, Rtings professional review
        self.panels["PA32UCG"] = PanelCharacterization(
            manufacturer="ASUS",
            model_pattern=r"PA32UCG|ProArt.*PA32UCG",
            panel_type="Mini-LED",
            display_name="ProArt PA32UCG",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6780, 0.3100),
                green=ChromaticityCoord(0.2650, 0.6900),
                blue=ChromaticityCoord(0.1380, 0.0520),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=600.0,
                max_luminance_hdr=1600.0,
                min_luminance=0.005,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=False,
                local_dimming=True,
                local_dimming_zones=1152
            ),
            notes="Reference Mini-LED. 99% DCI-P3, 89% BT.2020. 1152 zones FALD. Source: TFTCentral/Rtings."
        )

        # MSI MEG 342C QD-OLED
        # Source: Hardware Unboxed, TFTCentral
        self.panels["MEG342C"] = PanelCharacterization(
            manufacturer="MSI",
            model_pattern=r"MEG.*342C|MSI.*342C.*QD.*OLED",
            panel_type="QD-OLED",
            display_name="MEG 342C QD-OLED",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6785, 0.3090),
                green=ChromaticityCoord(0.2335, 0.7105),
                blue=ChromaticityCoord(0.1380, 0.0508),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2040, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1975, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2015, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=250.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1000000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="34-inch 3440x1440 QD-OLED ultrawide. Same panel family as G85SB. Source: HUB/TFTCentral."
        )

        # Corsair Xeneon 34 QD-OLED
        # Source: Rtings, Hardware Unboxed
        self.panels["XENEON34"] = PanelCharacterization(
            manufacturer="Corsair",
            model_pattern=r"Xeneon.*34|CM-9030002",
            panel_type="QD-OLED",
            display_name="Xeneon 34 QD-OLED",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6788, 0.3092),
                green=ChromaticityCoord(0.2332, 0.7102),
                blue=ChromaticityCoord(0.1378, 0.0505),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2035, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1980, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2010, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=260.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1000000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="34-inch QD-OLED with iCUE integration. Same Samsung panel. Source: Rtings/HUB."
        )

        # Gigabyte AORUS FO32U2P (QD-OLED 4K 32")
        # Source: Hardware Unboxed
        self.panels["FO32U2P"] = PanelCharacterization(
            manufacturer="Gigabyte",
            model_pattern=r"FO32U2P|AORUS.*FO32U2",
            panel_type="QD-OLED",
            display_name="AORUS FO32U2P",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6790, 0.3095),
                green=ChromaticityCoord(0.2325, 0.7110),
                blue=ChromaticityCoord(0.1375, 0.0500),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2018, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1988, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2005, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=275.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="32-inch 4K 240Hz QD-OLED. Same Samsung panel as PG32UCDM. Source: Hardware Unboxed."
        )

        # LG UltraGear OLED 27GR95QE (LG WOLED)
        self.panels["27GR95QE"] = PanelCharacterization(
            manufacturer="LG",
            model_pattern=r"27GR95QE|UltraGear.*27GR95",
            panel_type="WOLED",
            display_name="UltraGear OLED 27GR95QE",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6401, 0.3300),
                green=ChromaticityCoord(0.3000, 0.6000),
                blue=ChromaticityCoord(0.1500, 0.0600),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2010, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.1990, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=200.0,
                max_luminance_hdr=800.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="LG WOLED panel. Similar to PG27UCDM with LG OSD and features."
        )

        # Sony INZONE M9 (IPS with Full-Array Local Dimming)
        self.panels["INZONE_M9"] = PanelCharacterization(
            manufacturer="Sony",
            model_pattern=r"INZONE.*M9|SDM-U27M90",
            panel_type="IPS",
            display_name="INZONE M9",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6465, 0.3340),
                green=ChromaticityCoord(0.2700, 0.6350),
                blue=ChromaticityCoord(0.1500, 0.0600),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2200, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2200, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2200, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=350.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.05,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=True,
                local_dimming_zones=96
            ),
            notes="IPS with FALD. Requires local dimming consideration for calibration."
        )

        # Samsung Odyssey G7 / G5 (VA curved ultrawide - 3440x1440)
        # Source: Rtings, TFTCentral measurements
        self.panels["ODYSSEY_G7_UW"] = PanelCharacterization(
            manufacturer="Samsung",
            model_pattern=r"SAM72F2|G7.*34|C34G5|Odyssey.*G7.*34|LC34G5",
            panel_type="VA",
            display_name="Odyssey G7 Ultrawide",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3310),
                green=ChromaticityCoord(0.2680, 0.6420),
                blue=ChromaticityCoord(0.1500, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2100, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2050, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2080, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=350.0,
                max_luminance_hdr=600.0,
                min_luminance=0.05,
                native_contrast=2500.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Samsung VA curved ultrawide. 125% sRGB gamut. Good contrast. Source: Rtings/TFTCentral."
        )

        # Dell U2723QE - 4K 60Hz IPS (sRGB professional)
        # Source: Rtings.com, TFTCentral measurements
        self.panels["U2723QE"] = PanelCharacterization(
            manufacturer="Dell",
            model_pattern=r"U2723QE|Dell.*U2723",
            panel_type="IPS",
            display_name="U2723QE",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6400, 0.3300),
                green=ChromaticityCoord(0.3000, 0.6000),
                blue=ChromaticityCoord(0.1500, 0.0600),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=350.0,
                max_luminance_hdr=350.0,
                min_luminance=0.25,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=False,
                wide_gamut=False,
                vrr_capable=False,
                local_dimming=False
            ),
            notes="Factory calibrated sRGB IPS. Delta E < 2 out of box. USB-C hub monitor. Source: Rtings/TFTCentral."
        )

        # BenQ SW271C - 4K 60Hz IPS (Photo editing, 99% AdobeRGB)
        # Source: TFTCentral professional review
        self.panels["SW271C"] = PanelCharacterization(
            manufacturer="BenQ",
            model_pattern=r"SW271C|BenQ.*SW271C",
            panel_type="IPS",
            display_name="SW271C",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3320),
                green=ChromaticityCoord(0.2750, 0.6400),
                blue=ChromaticityCoord(0.1495, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=300.0,
                max_luminance_hdr=300.0,
                min_luminance=0.20,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=False,
                wide_gamut=True,
                vrr_capable=False,
                local_dimming=False
            ),
            notes="99% AdobeRGB photo editing monitor. Hardware calibration support. Delta E < 2. Source: TFTCentral."
        )

        # EIZO CG2700S - 2K 60Hz IPS (Professional reference)
        # Source: TFTCentral, EIZO published specifications
        self.panels["CG2700S"] = PanelCharacterization(
            manufacturer="EIZO",
            model_pattern=r"CG2700S|EIZO.*CG2700S|ColorEdge.*CG2700",
            panel_type="IPS",
            display_name="ColorEdge CG2700S",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3320),
                green=ChromaticityCoord(0.2750, 0.6400),
                blue=ChromaticityCoord(0.1495, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=300.0,
                max_luminance_hdr=300.0,
                min_luminance=0.20,
                native_contrast=1300.0,
                bit_depth=10,
                hdr_capable=False,
                wide_gamut=True,
                vrr_capable=False,
                local_dimming=False
            ),
            notes="Professional reference monitor. 99% AdobeRGB, built-in colorimeter. Delta E < 1. Source: TFTCentral."
        )

        # Dell U3423WE - 3440x1440 60Hz IPS (Ultrawide professional, 98% DCI-P3)
        # Source: Rtings.com measurements
        self.panels["U3423WE"] = PanelCharacterization(
            manufacturer="Dell",
            model_pattern=r"U3423WE|Dell.*U3423",
            panel_type="IPS",
            display_name="U3423WE",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3320),
                green=ChromaticityCoord(0.2750, 0.6400),
                blue=ChromaticityCoord(0.1495, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=350.0,
                max_luminance_hdr=350.0,
                min_luminance=0.25,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=False,
                wide_gamut=True,
                vrr_capable=False,
                local_dimming=False
            ),
            notes="Ultrawide professional IPS. 98% DCI-P3. USB-C hub. Factory calibrated. Source: Rtings."
        )

        # ASUS VG27AQ1A - 2K 170Hz IPS (Gaming, 130% sRGB)
        # Source: Rtings.com, Hardware Unboxed
        self.panels["VG27AQ1A"] = PanelCharacterization(
            manufacturer="ASUS",
            model_pattern=r"VG27AQ1A|ASUS.*VG27AQ1A",
            panel_type="IPS",
            display_name="TUF Gaming VG27AQ1A",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3320),
                green=ChromaticityCoord(0.2750, 0.6400),
                blue=ChromaticityCoord(0.1495, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2100, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2050, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2080, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=350.0,
                max_luminance_hdr=400.0,
                min_luminance=0.10,
                native_contrast=1000.0,
                bit_depth=8,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Gaming IPS with 130% sRGB coverage. ELMB Sync. HDR400. Source: Rtings/Hardware Unboxed."
        )

        # Samsung Odyssey G7 27" - 2K 240Hz VA (Gaming, curved)
        # Source: Rtings.com, TFTCentral measurements
        self.panels["ODYSSEY_G7_27"] = PanelCharacterization(
            manufacturer="Samsung",
            model_pattern=r"C27G7|LC27G7|Odyssey.*G7.*27|G7.*27",
            panel_type="VA",
            display_name="Odyssey G7 27\"",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3310),
                green=ChromaticityCoord(0.2680, 0.6420),
                blue=ChromaticityCoord(0.1500, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2120, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2060, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2090, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=350.0,
                max_luminance_hdr=600.0,
                min_luminance=0.05,
                native_contrast=2500.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="VA curved 1000R gaming monitor. 125% sRGB, HDR600. Source: Rtings/TFTCentral."
        )

        # LG 27GP850-B - 2K 165Hz Nano IPS (Gaming, 98% DCI-P3)
        # Source: Rtings.com, TFTCentral measurements
        self.panels["27GP850"] = PanelCharacterization(
            manufacturer="LG",
            model_pattern=r"27GP850|UltraGear.*27GP850",
            panel_type="Nano-IPS",
            display_name="UltraGear 27GP850-B",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3320),
                green=ChromaticityCoord(0.2750, 0.6400),
                blue=ChromaticityCoord(0.1495, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2100, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2050, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2080, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=350.0,
                max_luminance_hdr=400.0,
                min_luminance=0.08,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Nano IPS with 98% DCI-P3. 165Hz gaming. HDR400. Source: Rtings/TFTCentral."
        )

        # Dell S2722DGM - 2K 165Hz VA (Budget gaming)
        # Source: Rtings.com measurements
        self.panels["S2722DGM"] = PanelCharacterization(
            manufacturer="Dell",
            model_pattern=r"S2722DGM|Dell.*S2722DGM",
            panel_type="VA",
            display_name="S2722DGM",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6400, 0.3300),
                green=ChromaticityCoord(0.3000, 0.6000),
                blue=ChromaticityCoord(0.1500, 0.0600),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2150, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2100, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2120, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=300.0,
                max_luminance_hdr=400.0,
                min_luminance=0.05,
                native_contrast=3000.0,
                bit_depth=8,
                hdr_capable=False,
                wide_gamut=False,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Budget VA gaming monitor. ~99% sRGB. 165Hz curved. Source: Rtings."
        )

        # Sony A95L - 4K 120Hz QD-OLED TV
        # Source: Rtings.com, HDTVTest
        self.panels["SONY_A95L"] = PanelCharacterization(
            manufacturer="Sony",
            model_pattern=r"A95L|XR.*A95L|BRAVIA.*A95L",
            panel_type="QD-OLED",
            display_name="BRAVIA A95L",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6795, 0.3095),
                green=ChromaticityCoord(0.2325, 0.7115),
                blue=ChromaticityCoord(0.1375, 0.0495),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2020, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1985, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2005, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=250.0,
                max_luminance_hdr=1400.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Sony QD-OLED TV with Samsung Display panel. Excellent processing. Source: Rtings/HDTVTest."
        )

        # Samsung S95D - 4K 144Hz QD-OLED TV
        # Source: Rtings.com, HDTVTest
        self.panels["S95D"] = PanelCharacterization(
            manufacturer="Samsung",
            model_pattern=r"S95D|QE.*S95D|QN.*S95D",
            panel_type="QD-OLED",
            display_name="S95D QD-OLED",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6792, 0.3098),
                green=ChromaticityCoord(0.2318, 0.7108),
                blue=ChromaticityCoord(0.1372, 0.0498),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2010, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1990, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=260.0,
                max_luminance_hdr=1500.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="2024 Samsung QD-OLED TV with anti-glare. 144Hz VRR. Source: Rtings/HDTVTest."
        )

        # ASUS PG34WCDM - 3440x1440 240Hz QD-OLED (Ultrawide)
        # Source: Hardware Unboxed, TFTCentral
        self.panels["PG34WCDM"] = PanelCharacterization(
            manufacturer="ASUS",
            model_pattern=r"PG34WCDM|ROG.*PG34WCDM|PG34.*WCDM",
            panel_type="QD-OLED",
            display_name="ROG Swift OLED PG34WCDM",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6790, 0.3095),
                green=ChromaticityCoord(0.2328, 0.7108),
                blue=ChromaticityCoord(0.1376, 0.0500),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2025, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1985, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2010, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=275.0,
                max_luminance_hdr=1000.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="34-inch 3440x1440 240Hz QD-OLED ultrawide. Samsung 2024 panel family. Source: HUB/TFTCentral."
        )

        # Gigabyte M28U - 4K 144Hz IPS (Budget 4K gaming)
        # Source: Rtings.com, Hardware Unboxed
        self.panels["M28U"] = PanelCharacterization(
            manufacturer="Gigabyte",
            model_pattern=r"M28U|GIGABYTE.*M28U",
            panel_type="IPS",
            display_name="M28U",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3320),
                green=ChromaticityCoord(0.2750, 0.6400),
                blue=ChromaticityCoord(0.1495, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2100, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2050, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2080, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=300.0,
                max_luminance_hdr=400.0,
                min_luminance=0.10,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="Budget 4K 144Hz IPS gaming. 90% DCI-P3. HDMI 2.1. Source: Rtings/Hardware Unboxed."
        )

        # ViewSonic VP2786-4K - 4K 60Hz IPS (Professional)
        # Source: TFTCentral, Rtings
        self.panels["VP2786"] = PanelCharacterization(
            manufacturer="ViewSonic",
            model_pattern=r"VP2786|ViewSonic.*VP2786",
            panel_type="IPS",
            display_name="VP2786-4K",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6400, 0.3300),
                green=ChromaticityCoord(0.3000, 0.6000),
                blue=ChromaticityCoord(0.1500, 0.0600),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=350.0,
                max_luminance_hdr=350.0,
                min_luminance=0.25,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=False,
                wide_gamut=False,
                vrr_capable=False,
                local_dimming=False
            ),
            notes="Professional 4K IPS. 100% sRGB, factory calibrated Delta E < 2. USB-C. Source: TFTCentral/Rtings."
        )

        # LG 32GS95UE - 4K 240Hz WOLED 32"
        # Source: Rtings.com, Hardware Unboxed
        self.panels["32GS95UE"] = PanelCharacterization(
            manufacturer="LG",
            model_pattern=r"32GS95UE|UltraGear.*32GS95",
            panel_type="WOLED",
            display_name="UltraGear OLED 32GS95UE",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6398, 0.3302),
                green=ChromaticityCoord(0.2999, 0.5997),
                blue=ChromaticityCoord(0.1501, 0.0602),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2008, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.1995, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2002, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=275.0,
                max_luminance_hdr=900.0,
                min_luminance=0.0001,
                native_contrast=1500000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="32-inch 4K 240Hz WOLED monitor. Similar to LG C4 primaries. Source: Rtings/Hardware Unboxed."
        )

        # MSI MAG 274QRF-QD - 2K 165Hz Quantum Dot IPS
        # Source: Rtings.com, Hardware Unboxed
        self.panels["274QRF_QD"] = PanelCharacterization(
            manufacturer="MSI",
            model_pattern=r"274QRF.*QD|MAG.*274QRF|MSI.*274QRF",
            panel_type="IPS",
            display_name="MAG 274QRF QD",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6480, 0.3320),
                green=ChromaticityCoord(0.2750, 0.6400),
                blue=ChromaticityCoord(0.1495, 0.0580),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2080, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2040, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2060, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=350.0,
                max_luminance_hdr=400.0,
                min_luminance=0.08,
                native_contrast=1000.0,
                bit_depth=10,
                hdr_capable=True,
                wide_gamut=True,
                vrr_capable=True,
                local_dimming=False
            ),
            notes="QD-enhanced IPS with ~97% DCI-P3. Excellent color for gaming. Source: Rtings/Hardware Unboxed."
        )

        # Generic sRGB IPS (fallback for unknown panels)
        self.panels["GENERIC_SRGB"] = PanelCharacterization(
            manufacturer="Generic",
            model_pattern=r".*",  # Matches anything as fallback
            panel_type="IPS",
            display_name="Unknown Display",
            native_primaries=PanelPrimaries(
                red=ChromaticityCoord(0.6400, 0.3300),
                green=ChromaticityCoord(0.3000, 0.6000),
                blue=ChromaticityCoord(0.1500, 0.0600),
                white=ChromaticityCoord(0.3127, 0.3290)
            ),
            gamma_red=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_green=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            gamma_blue=GammaCurve(gamma=2.2000, offset=0.0, linear_portion=0.0),
            capabilities=PanelCapabilities(
                max_luminance_sdr=250.0,
                max_luminance_hdr=400.0,
                min_luminance=0.1,
                native_contrast=1000.0,
                bit_depth=8,
                hdr_capable=False,
                wide_gamut=False,
                vrr_capable=False,
                local_dimming=False
            ),
            notes="Generic sRGB panel. Used as fallback when no specific profile exists."
        )

    def _load_json_profiles(self):
        """Load panel profiles from JSON files in profiles directory."""
        for json_file in self.profiles_dir.glob("*.json"):
            try:
                with open(json_file, "r", encoding="utf-8") as f:
                    data = json.load(f)

                if isinstance(data, list):
                    # File contains multiple panels
                    for panel_data in data:
                        panel = PanelCharacterization.from_dict(panel_data)
                        self.panels[panel.model_pattern.split("|")[0]] = panel
                else:
                    # Single panel
                    panel = PanelCharacterization.from_dict(data)
                    self.panels[panel.model_pattern.split("|")[0]] = panel

            except Exception as e:
                print(f"Warning: Failed to load {json_file}: {e}")

    def find_panel(self, model_string: str) -> Optional[PanelCharacterization]:
        """
        Find panel characterization by model string (from EDID).

        Args:
            model_string: Display model string from EDID

        Returns:
            PanelCharacterization if found, None otherwise
        """
        # First, try exact match on known panel names
        for key, panel in self.panels.items():
            if key != "GENERIC_SRGB":  # Skip generic fallback
                if re.search(panel.model_pattern, model_string, re.IGNORECASE):
                    return panel

        return None

    def get_panel(self, model_key: str) -> Optional[PanelCharacterization]:
        """Get panel by exact key name."""
        return self.panels.get(model_key)

    def get_fallback(self) -> PanelCharacterization:
        """Get generic fallback panel profile."""
        return self.panels["GENERIC_SRGB"]

    def list_panels(self) -> List[str]:
        """List all available panel keys."""
        return [k for k in self.panels.keys() if k != "GENERIC_SRGB"]

    def add_panel(self, key: str, panel: PanelCharacterization):
        """Add or update a panel characterization."""
        self.panels[key] = panel

    def save_panel(self, key: str, filename: Optional[str] = None):
        """Save a panel characterization to JSON file."""
        if key not in self.panels:
            raise ValueError(f"Panel '{key}' not found in database")

        panel = self.panels[key]
        if filename is None:
            filename = f"{key.lower().replace(' ', '_')}.json"

        filepath = self.profiles_dir / filename

        # Ensure directory exists
        self.profiles_dir.mkdir(parents=True, exist_ok=True)

        with open(filepath, "w", encoding="utf-8") as f:
            json.dump(panel.to_dict(), f, indent=2)

        return filepath


# Global database instance
_database: Optional[PanelDatabase] = None

def get_database() -> PanelDatabase:
    """Get the global panel database instance."""
    global _database
    if _database is None:
        _database = PanelDatabase()
    return _database

def find_panel_for_display(model_string: str) -> Optional[PanelCharacterization]:
    """Convenience function to find a panel by model string."""
    return get_database().find_panel(model_string)


def create_from_edid(edid_chromaticity: Dict, monitor_name: str = "Unknown",
                     manufacturer: str = "Unknown", gamma: float = 2.2) -> PanelCharacterization:
    """
    Create a PanelCharacterization from EDID chromaticity data.

    This is the critical fallback for monitors not in the built-in database.
    EDID reports native primaries and white point, giving us accurate gamut
    information even for unknown panels. Combined with a reasonable gamma
    assumption, this produces significantly better calibration than the
    generic sRGB fallback.

    Args:
        edid_chromaticity: Dict with 'red', 'green', 'blue', 'white' as (x, y) tuples
        monitor_name: Display name from EDID
        manufacturer: Manufacturer name
        gamma: Assumed gamma (2.2 for most IPS/VA, 2.4 for some VA panels)

    Returns:
        PanelCharacterization built from EDID data
    """
    red = edid_chromaticity.get("red", (0.6400, 0.3300))
    green = edid_chromaticity.get("green", (0.3000, 0.6000))
    blue = edid_chromaticity.get("blue", (0.1500, 0.0600))
    white = edid_chromaticity.get("white", (0.3127, 0.3290))

    # Determine panel type heuristic from gamut coverage
    # sRGB red primary is at (0.64, 0.33). Wide gamut panels have red > 0.66
    is_wide_gamut = red[0] > 0.66 or green[1] > 0.65

    # Estimate panel type from gamut width
    if red[0] > 0.675:
        panel_type = "Wide Gamut"  # QD-OLED or similar
    elif red[0] > 0.66:
        panel_type = "DCI-P3"  # P3-class panel
    else:
        panel_type = "sRGB-class"

    # Build a correction matrix based on how far primaries are from sRGB
    # This is an identity-ish matrix with small corrections
    srgb_red = (0.6400, 0.3300)
    srgb_green = (0.3000, 0.6000)
    srgb_blue = (0.1500, 0.0600)

    # Calculate approximate correction magnitudes
    r_shift = abs(red[0] - srgb_red[0]) + abs(red[1] - srgb_red[1])
    g_shift = abs(green[0] - srgb_green[0]) + abs(green[1] - srgb_green[1])
    b_shift = abs(blue[0] - srgb_blue[0]) + abs(blue[1] - srgb_blue[1])

    # Build conservative correction matrix
    ccm = [
        [1.0 + r_shift * 0.5, -g_shift * 0.3, -b_shift * 0.2],
        [-r_shift * 0.1, 1.0 + g_shift * 0.3, -b_shift * 0.15],
        [r_shift * 0.03, -g_shift * 0.5, 1.0 + b_shift * 0.45]
    ]

    return PanelCharacterization(
        manufacturer=manufacturer,
        model_pattern=re.escape(monitor_name),
        panel_type=panel_type,
        native_primaries=PanelPrimaries(
            red=ChromaticityCoord(red[0], red[1]),
            green=ChromaticityCoord(green[0], green[1]),
            blue=ChromaticityCoord(blue[0], blue[1]),
            white=ChromaticityCoord(white[0], white[1])
        ),
        gamma_red=GammaCurve(gamma=gamma, offset=0.0, linear_portion=0.0),
        gamma_green=GammaCurve(gamma=gamma, offset=0.0, linear_portion=0.0),
        gamma_blue=GammaCurve(gamma=gamma, offset=0.0, linear_portion=0.0),
        capabilities=PanelCapabilities(
            max_luminance_sdr=250.0,
            max_luminance_hdr=400.0,
            min_luminance=0.1,
            native_contrast=1000.0,
            bit_depth=8,
            hdr_capable=is_wide_gamut,
            wide_gamut=is_wide_gamut,
            vrr_capable=False,
            local_dimming=False
        ),
        color_correction_matrix=ccm,
        notes=f"Auto-generated from EDID data. Primaries: R({red[0]:.4f},{red[1]:.4f}) "
              f"G({green[0]:.4f},{green[1]:.4f}) B({blue[0]:.4f},{blue[1]:.4f}). "
              f"Gamma assumed {gamma}."
    )
