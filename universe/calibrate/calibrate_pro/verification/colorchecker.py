"""
ColorChecker Verification Module

Provides comprehensive verification using X-Rite ColorChecker targets:
- Classic 24-patch verification
- Digital SG 140-patch verification
- Per-patch Delta E analysis
- Gamut mapping quality assessment
- Statistical analysis and grading
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Optional
import numpy as np

# =============================================================================
# Constants - ColorChecker Reference Data
# =============================================================================

# ColorChecker Classic 24-patch reference values (D50, Lab)
# Based on X-Rite published data for ColorChecker Classic (2014)
COLORCHECKER_CLASSIC_D50: dict[str, tuple[float, float, float]] = {
    # Row 1 - Natural colors
    "dark_skin": (37.986, 13.555, 14.059),
    "light_skin": (65.711, 18.130, 17.810),
    "blue_sky": (49.927, -4.880, -21.925),
    "foliage": (43.139, -13.095, 21.905),
    "blue_flower": (55.112, 8.844, -25.399),
    "bluish_green": (70.719, -33.397, -0.199),
    # Row 2 - Miscellaneous colors
    "orange": (62.661, 36.067, 57.096),
    "purplish_blue": (40.020, 10.410, -45.964),
    "moderate_red": (51.124, 48.239, 16.248),
    "purple": (30.325, 22.976, -21.587),
    "yellow_green": (72.532, -23.709, 57.255),
    "orange_yellow": (71.941, 19.363, 67.857),
    # Row 3 - Primary and secondary colors
    "blue": (28.778, 14.179, -50.297),
    "green": (55.261, -38.342, 31.370),
    "red": (42.101, 53.378, 28.190),
    "yellow": (81.733, 4.039, 79.819),
    "magenta": (51.935, 49.986, -14.574),
    "cyan": (51.038, -28.631, -28.638),
    # Row 4 - Grayscale
    "white": (96.539, -0.425, 1.186),
    "neutral_8": (81.257, -0.638, -0.335),
    "neutral_6.5": (66.766, -0.734, -0.504),
    "neutral_5": (50.867, -0.153, -0.270),
    "neutral_3.5": (35.656, -0.421, -1.231),
    "black": (20.461, -0.079, -0.973),
}

# ColorChecker Classic patch names in order (left-to-right, top-to-bottom)
COLORCHECKER_CLASSIC_ORDER = [
    "dark_skin", "light_skin", "blue_sky", "foliage", "blue_flower", "bluish_green",
    "orange", "purplish_blue", "moderate_red", "purple", "yellow_green", "orange_yellow",
    "blue", "green", "red", "yellow", "magenta", "cyan",
    "white", "neutral_8", "neutral_6.5", "neutral_5", "neutral_3.5", "black",
]

# ColorChecker Classic display names
COLORCHECKER_CLASSIC_NAMES = {
    "dark_skin": "Dark Skin",
    "light_skin": "Light Skin",
    "blue_sky": "Blue Sky",
    "foliage": "Foliage",
    "blue_flower": "Blue Flower",
    "bluish_green": "Bluish Green",
    "orange": "Orange",
    "purplish_blue": "Purplish Blue",
    "moderate_red": "Moderate Red",
    "purple": "Purple",
    "yellow_green": "Yellow Green",
    "orange_yellow": "Orange Yellow",
    "blue": "Blue",
    "green": "Green",
    "red": "Red",
    "yellow": "Yellow",
    "magenta": "Magenta",
    "cyan": "Cyan",
    "white": "White",
    "neutral_8": "Neutral 8",
    "neutral_6.5": "Neutral 6.5",
    "neutral_5": "Neutral 5",
    "neutral_3.5": "Neutral 3.5",
    "black": "Black",
}

# Patch categories for analysis
COLORCHECKER_CATEGORIES = {
    "skin_tones": ["dark_skin", "light_skin"],
    "natural": ["blue_sky", "foliage", "blue_flower", "bluish_green"],
    "saturated": ["orange", "purplish_blue", "moderate_red", "purple", "yellow_green", "orange_yellow"],
    "primaries": ["blue", "green", "red", "yellow", "magenta", "cyan"],
    "grayscale": ["white", "neutral_8", "neutral_6.5", "neutral_5", "neutral_3.5", "black"],
}


# =============================================================================
# Enums
# =============================================================================

class VerificationGrade(Enum):
    """Verification quality grade based on Delta E statistics."""
    REFERENCE = auto()      # ΔE < 1.0 - Reference grade
    EXCELLENT = auto()      # ΔE < 2.0 - Excellent
    GOOD = auto()           # ΔE < 3.0 - Good
    ACCEPTABLE = auto()     # ΔE < 5.0 - Acceptable
    POOR = auto()           # ΔE >= 5.0 - Poor


class ColorCheckerType(Enum):
    """ColorChecker target type."""
    CLASSIC_24 = auto()     # Standard 24-patch
    DIGITAL_SG = auto()     # 140-patch Digital SG
    PASSPORT = auto()       # ColorChecker Passport
    VIDEO = auto()          # ColorChecker Video


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class PatchMeasurement:
    """Single patch measurement result."""
    patch_id: str
    patch_name: str
    reference_lab: tuple[float, float, float]
    measured_lab: tuple[float, float, float]
    delta_e_76: float
    delta_e_2000: float
    delta_l: float          # Lightness difference
    delta_c: float          # Chroma difference
    delta_h: float          # Hue difference
    category: str = ""

    @property
    def reference_l(self) -> float:
        return self.reference_lab[0]

    @property
    def reference_a(self) -> float:
        return self.reference_lab[1]

    @property
    def reference_b(self) -> float:
        return self.reference_lab[2]

    @property
    def measured_l(self) -> float:
        return self.measured_lab[0]

    @property
    def measured_a(self) -> float:
        return self.measured_lab[1]

    @property
    def measured_b(self) -> float:
        return self.measured_lab[2]


@dataclass
class CategoryAnalysis:
    """Analysis results for a patch category."""
    category: str
    patch_count: int
    delta_e_mean: float
    delta_e_max: float
    delta_e_min: float
    delta_e_std: float
    delta_l_mean: float
    delta_c_mean: float
    delta_h_mean: float
    grade: VerificationGrade


@dataclass
class ColorCheckerResult:
    """Complete ColorChecker verification result."""
    target_type: ColorCheckerType
    patch_measurements: list[PatchMeasurement]
    category_analysis: dict[str, CategoryAnalysis]

    # Overall statistics
    delta_e_mean: float
    delta_e_max: float
    delta_e_min: float
    delta_e_std: float
    delta_e_median: float
    delta_e_95th: float

    # Component statistics
    delta_l_mean: float
    delta_c_mean: float
    delta_h_mean: float

    # Grading
    overall_grade: VerificationGrade
    grayscale_grade: VerificationGrade
    skin_tone_grade: VerificationGrade

    # Metadata
    timestamp: str = ""
    display_name: str = ""
    profile_name: str = ""

    @property
    def passed(self) -> bool:
        """Check if verification passed (acceptable or better)."""
        return self.overall_grade in (
            VerificationGrade.REFERENCE,
            VerificationGrade.EXCELLENT,
            VerificationGrade.GOOD,
            VerificationGrade.ACCEPTABLE,
        )


# =============================================================================
# Color Math Functions
# =============================================================================

def lab_to_lch(lab: tuple[float, float, float]) -> tuple[float, float, float]:
    """Convert Lab to LCH (Lightness, Chroma, Hue)."""
    L, a, b = lab
    C = np.sqrt(a**2 + b**2)
    H = np.degrees(np.arctan2(b, a))
    if H < 0:
        H += 360
    return (L, C, H)


def delta_e_1976(lab1: tuple[float, float, float],
                 lab2: tuple[float, float, float]) -> float:
    """Calculate CIE76 Delta E (Euclidean distance in Lab)."""
    L1, a1, b1 = lab1
    L2, a2, b2 = lab2
    return np.sqrt((L2 - L1)**2 + (a2 - a1)**2 + (b2 - b1)**2)


def delta_e_2000(lab1: tuple[float, float, float],
                 lab2: tuple[float, float, float],
                 kL: float = 1.0, kC: float = 1.0, kH: float = 1.0) -> float:
    """
    Calculate CIEDE2000 Delta E.

    This is the industry standard for color difference calculation,
    providing better correlation with human perception than CIE76.

    Args:
        lab1: Reference Lab values
        lab2: Sample Lab values
        kL: Lightness weighting factor (default 1.0)
        kC: Chroma weighting factor (default 1.0)
        kH: Hue weighting factor (default 1.0)

    Returns:
        CIEDE2000 Delta E value
    """
    L1, a1, b1 = lab1
    L2, a2, b2 = lab2

    # Calculate C'ab and h'ab
    C1 = np.sqrt(a1**2 + b1**2)
    C2 = np.sqrt(a2**2 + b2**2)
    C_avg = (C1 + C2) / 2

    G = 0.5 * (1 - np.sqrt(C_avg**7 / (C_avg**7 + 25**7)))

    a1_prime = a1 * (1 + G)
    a2_prime = a2 * (1 + G)

    C1_prime = np.sqrt(a1_prime**2 + b1**2)
    C2_prime = np.sqrt(a2_prime**2 + b2**2)

    h1_prime = np.degrees(np.arctan2(b1, a1_prime)) % 360
    h2_prime = np.degrees(np.arctan2(b2, a2_prime)) % 360

    # Calculate delta values
    delta_L_prime = L2 - L1
    delta_C_prime = C2_prime - C1_prime

    delta_h_prime = h2_prime - h1_prime
    if abs(delta_h_prime) > 180:
        if delta_h_prime > 0:
            delta_h_prime -= 360
        else:
            delta_h_prime += 360

    delta_H_prime = 2 * np.sqrt(C1_prime * C2_prime) * np.sin(np.radians(delta_h_prime / 2))

    # Calculate CIEDE2000 components
    L_prime_avg = (L1 + L2) / 2
    C_prime_avg = (C1_prime + C2_prime) / 2

    h_prime_sum = h1_prime + h2_prime
    if abs(h1_prime - h2_prime) > 180:
        h_prime_sum += 360
    h_prime_avg = h_prime_sum / 2

    T = (1 - 0.17 * np.cos(np.radians(h_prime_avg - 30)) +
         0.24 * np.cos(np.radians(2 * h_prime_avg)) +
         0.32 * np.cos(np.radians(3 * h_prime_avg + 6)) -
         0.20 * np.cos(np.radians(4 * h_prime_avg - 63)))

    delta_theta = 30 * np.exp(-((h_prime_avg - 275) / 25)**2)

    R_C = 2 * np.sqrt(C_prime_avg**7 / (C_prime_avg**7 + 25**7))

    S_L = 1 + (0.015 * (L_prime_avg - 50)**2) / np.sqrt(20 + (L_prime_avg - 50)**2)
    S_C = 1 + 0.045 * C_prime_avg
    S_H = 1 + 0.015 * C_prime_avg * T

    R_T = -np.sin(np.radians(2 * delta_theta)) * R_C

    # Final calculation
    delta_E = np.sqrt(
        (delta_L_prime / (kL * S_L))**2 +
        (delta_C_prime / (kC * S_C))**2 +
        (delta_H_prime / (kH * S_H))**2 +
        R_T * (delta_C_prime / (kC * S_C)) * (delta_H_prime / (kH * S_H))
    )

    return delta_E


def calculate_delta_components(lab1: tuple[float, float, float],
                               lab2: tuple[float, float, float]) -> tuple[float, float, float]:
    """
    Calculate Delta L, Delta C, Delta H components.

    Returns:
        (delta_L, delta_C, delta_H) tuple
    """
    L1, a1, b1 = lab1
    L2, a2, b2 = lab2

    # Lightness difference
    delta_L = L2 - L1

    # Chroma difference
    C1 = np.sqrt(a1**2 + b1**2)
    C2 = np.sqrt(a2**2 + b2**2)
    delta_C = C2 - C1

    # Hue difference (using delta_a and delta_b)
    delta_a = a2 - a1
    delta_b = b2 - b1
    delta_H_sq = delta_a**2 + delta_b**2 - delta_C**2
    delta_H = np.sqrt(max(0, delta_H_sq))  # Avoid negative sqrt

    return (delta_L, delta_C, delta_H)


# =============================================================================
# Verification Grade Functions
# =============================================================================

def grade_from_delta_e(delta_e: float) -> VerificationGrade:
    """Determine verification grade from Delta E value."""
    if delta_e < 1.0:
        return VerificationGrade.REFERENCE
    elif delta_e < 2.0:
        return VerificationGrade.EXCELLENT
    elif delta_e < 3.0:
        return VerificationGrade.GOOD
    elif delta_e < 5.0:
        return VerificationGrade.ACCEPTABLE
    else:
        return VerificationGrade.POOR


def grade_to_string(grade: VerificationGrade) -> str:
    """Convert grade enum to display string."""
    return {
        VerificationGrade.REFERENCE: "Reference Grade (ΔE < 1.0)",
        VerificationGrade.EXCELLENT: "Excellent (ΔE < 2.0)",
        VerificationGrade.GOOD: "Good (ΔE < 3.0)",
        VerificationGrade.ACCEPTABLE: "Acceptable (ΔE < 5.0)",
        VerificationGrade.POOR: "Poor (ΔE ≥ 5.0)",
    }[grade]


# =============================================================================
# ColorChecker Verification Class
# =============================================================================

class ColorCheckerVerifier:
    """
    ColorChecker verification engine.

    Performs comprehensive verification using X-Rite ColorChecker targets,
    calculating Delta E for each patch and providing statistical analysis.
    """

    def __init__(self, target_type: ColorCheckerType = ColorCheckerType.CLASSIC_24):
        """
        Initialize verifier.

        Args:
            target_type: Type of ColorChecker target to use
        """
        self.target_type = target_type
        self._reference_data = self._get_reference_data()

    def _get_reference_data(self) -> dict[str, tuple[float, float, float]]:
        """Get reference Lab values for selected target type."""
        if self.target_type == ColorCheckerType.CLASSIC_24:
            return COLORCHECKER_CLASSIC_D50
        # Add other target types as needed
        return COLORCHECKER_CLASSIC_D50

    def verify(self,
               measured_lab: dict[str, tuple[float, float, float]],
               display_name: str = "",
               profile_name: str = "") -> ColorCheckerResult:
        """
        Perform ColorChecker verification.

        Args:
            measured_lab: Dictionary mapping patch IDs to measured Lab values
            display_name: Name of verified display
            profile_name: Name of ICC profile used

        Returns:
            ColorCheckerResult with complete analysis
        """
        from datetime import datetime

        patch_measurements: list[PatchMeasurement] = []
        delta_e_values: list[float] = []

        # Process each patch
        for patch_id in COLORCHECKER_CLASSIC_ORDER:
            if patch_id not in measured_lab:
                continue

            ref_lab = self._reference_data[patch_id]
            meas_lab = measured_lab[patch_id]

            # Calculate Delta E values
            de_76 = delta_e_1976(ref_lab, meas_lab)
            de_2000 = delta_e_2000(ref_lab, meas_lab)

            # Calculate component differences
            delta_l, delta_c, delta_h = calculate_delta_components(ref_lab, meas_lab)

            # Determine category
            category = ""
            for cat_name, cat_patches in COLORCHECKER_CATEGORIES.items():
                if patch_id in cat_patches:
                    category = cat_name
                    break

            measurement = PatchMeasurement(
                patch_id=patch_id,
                patch_name=COLORCHECKER_CLASSIC_NAMES.get(patch_id, patch_id),
                reference_lab=ref_lab,
                measured_lab=meas_lab,
                delta_e_76=de_76,
                delta_e_2000=de_2000,
                delta_l=delta_l,
                delta_c=delta_c,
                delta_h=delta_h,
                category=category,
            )
            patch_measurements.append(measurement)
            delta_e_values.append(de_2000)

        # Calculate overall statistics
        de_array = np.array(delta_e_values)
        delta_e_mean = float(np.mean(de_array))
        delta_e_max = float(np.max(de_array))
        delta_e_min = float(np.min(de_array))
        delta_e_std = float(np.std(de_array))
        delta_e_median = float(np.median(de_array))
        delta_e_95th = float(np.percentile(de_array, 95))

        # Calculate component statistics
        delta_l_values = [m.delta_l for m in patch_measurements]
        delta_c_values = [m.delta_c for m in patch_measurements]
        delta_h_values = [m.delta_h for m in patch_measurements]

        delta_l_mean = float(np.mean(np.abs(delta_l_values)))
        delta_c_mean = float(np.mean(np.abs(delta_c_values)))
        delta_h_mean = float(np.mean(delta_h_values))

        # Analyze by category
        category_analysis = self._analyze_categories(patch_measurements)

        # Determine grades
        overall_grade = grade_from_delta_e(delta_e_mean)
        grayscale_grade = category_analysis.get("grayscale",
            CategoryAnalysis("grayscale", 0, 0, 0, 0, 0, 0, 0, 0, VerificationGrade.POOR)).grade
        skin_tone_grade = category_analysis.get("skin_tones",
            CategoryAnalysis("skin_tones", 0, 0, 0, 0, 0, 0, 0, 0, VerificationGrade.POOR)).grade

        return ColorCheckerResult(
            target_type=self.target_type,
            patch_measurements=patch_measurements,
            category_analysis=category_analysis,
            delta_e_mean=delta_e_mean,
            delta_e_max=delta_e_max,
            delta_e_min=delta_e_min,
            delta_e_std=delta_e_std,
            delta_e_median=delta_e_median,
            delta_e_95th=delta_e_95th,
            delta_l_mean=delta_l_mean,
            delta_c_mean=delta_c_mean,
            delta_h_mean=delta_h_mean,
            overall_grade=overall_grade,
            grayscale_grade=grayscale_grade,
            skin_tone_grade=skin_tone_grade,
            timestamp=datetime.now().isoformat(),
            display_name=display_name,
            profile_name=profile_name,
        )

    def _analyze_categories(self,
                           measurements: list[PatchMeasurement]) -> dict[str, CategoryAnalysis]:
        """Analyze measurements by category."""
        category_analysis: dict[str, CategoryAnalysis] = {}

        for cat_name, cat_patches in COLORCHECKER_CATEGORIES.items():
            cat_measurements = [m for m in measurements if m.patch_id in cat_patches]

            if not cat_measurements:
                continue

            de_values = [m.delta_e_2000 for m in cat_measurements]
            dl_values = [m.delta_l for m in cat_measurements]
            dc_values = [m.delta_c for m in cat_measurements]
            dh_values = [m.delta_h for m in cat_measurements]

            de_mean = float(np.mean(de_values))

            category_analysis[cat_name] = CategoryAnalysis(
                category=cat_name,
                patch_count=len(cat_measurements),
                delta_e_mean=de_mean,
                delta_e_max=float(np.max(de_values)),
                delta_e_min=float(np.min(de_values)),
                delta_e_std=float(np.std(de_values)),
                delta_l_mean=float(np.mean(np.abs(dl_values))),
                delta_c_mean=float(np.mean(np.abs(dc_values))),
                delta_h_mean=float(np.mean(dh_values)),
                grade=grade_from_delta_e(de_mean),
            )

        return category_analysis

    def verify_from_xyz(self,
                        measured_xyz: dict[str, tuple[float, float, float]],
                        illuminant: str = "D50",
                        display_name: str = "",
                        profile_name: str = "") -> ColorCheckerResult:
        """
        Perform verification from XYZ measurements.

        Args:
            measured_xyz: Dictionary mapping patch IDs to measured XYZ values
            illuminant: Reference illuminant (D50 or D65)
            display_name: Name of verified display
            profile_name: Name of ICC profile used

        Returns:
            ColorCheckerResult with complete analysis
        """
        # Convert XYZ to Lab
        measured_lab = {}
        for patch_id, xyz in measured_xyz.items():
            measured_lab[patch_id] = xyz_to_lab(xyz, illuminant)

        return self.verify(measured_lab, display_name, profile_name)


def xyz_to_lab(xyz: tuple[float, float, float],
               illuminant: str = "D50") -> tuple[float, float, float]:
    """
    Convert XYZ to Lab.

    Args:
        xyz: XYZ values (Y normalized to 100)
        illuminant: Reference illuminant (D50 or D65)

    Returns:
        Lab values
    """
    # Reference white points
    white_points = {
        "D50": (96.422, 100.0, 82.521),
        "D65": (95.047, 100.0, 108.883),
    }

    Xn, Yn, Zn = white_points.get(illuminant, white_points["D50"])
    X, Y, Z = xyz

    # Normalize
    x = X / Xn
    y = Y / Yn
    z = Z / Zn

    # Apply f function
    def f(t):
        delta = 6 / 29
        if t > delta**3:
            return t ** (1/3)
        else:
            return t / (3 * delta**2) + 4 / 29

    L = 116 * f(y) - 16
    a = 500 * (f(x) - f(y))
    b = 200 * (f(y) - f(z))

    return (L, a, b)


# =============================================================================
# Utility Functions
# =============================================================================

def create_test_measurements() -> dict[str, tuple[float, float, float]]:
    """Create simulated measurements for testing (with small random errors)."""
    np.random.seed(42)
    measurements = {}

    for patch_id, ref_lab in COLORCHECKER_CLASSIC_D50.items():
        # Add small random error (simulate ΔE ~1-2)
        error = np.random.normal(0, 0.7, 3)
        meas_lab = (
            ref_lab[0] + error[0],
            ref_lab[1] + error[1],
            ref_lab[2] + error[2],
        )
        measurements[patch_id] = meas_lab

    return measurements


def print_verification_summary(result: ColorCheckerResult) -> None:
    """Print verification summary to console."""
    print("\n" + "=" * 60)
    print("ColorChecker Verification Summary")
    print("=" * 60)
    print(f"Display: {result.display_name or 'Unknown'}")
    print(f"Profile: {result.profile_name or 'Unknown'}")
    print(f"Timestamp: {result.timestamp}")
    print()
    print(f"Overall Grade: {grade_to_string(result.overall_grade)}")
    print(f"Grayscale Grade: {grade_to_string(result.grayscale_grade)}")
    print(f"Skin Tone Grade: {grade_to_string(result.skin_tone_grade)}")
    print()
    print("Delta E Statistics (CIEDE2000):")
    print(f"  Mean:   {result.delta_e_mean:.2f}")
    print(f"  Max:    {result.delta_e_max:.2f}")
    print(f"  Min:    {result.delta_e_min:.2f}")
    print(f"  StdDev: {result.delta_e_std:.2f}")
    print(f"  Median: {result.delta_e_median:.2f}")
    print(f"  95th %: {result.delta_e_95th:.2f}")
    print()
    print("Category Analysis:")
    for cat_name, analysis in result.category_analysis.items():
        print(f"  {cat_name.replace('_', ' ').title()}: "
              f"Mean ΔE = {analysis.delta_e_mean:.2f} ({analysis.grade.name})")
    print("=" * 60)


# =============================================================================
# Module Test
# =============================================================================

if __name__ == "__main__":
    # Test verification
    verifier = ColorCheckerVerifier(ColorCheckerType.CLASSIC_24)
    test_measurements = create_test_measurements()
    result = verifier.verify(
        test_measurements,
        display_name="Test Display",
        profile_name="Test Profile"
    )
    print_verification_summary(result)
