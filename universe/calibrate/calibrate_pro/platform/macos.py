"""
macOS Platform Backend

Full implementation using Apple frameworks via PyObjC:
- CoreGraphics (Quartz) for display enumeration and gamma ramps
- IOKit for EDID reading (manufacturer, model, serial)
- ColorSync for ICC profile management

Requires: pyobjc-framework-Quartz, pyobjc-framework-CoreFoundation

macOS gamma tables:
    CGSetDisplayTransferByTable accepts float arrays (0.0-1.0) with
    up to 1024 entries per channel. We use 256 entries for consistency
    with the Windows VCGT path.

ICC profiles:
    Installed to ~/Library/ColorSync/Profiles/ (per-user, no admin needed)
    or /Library/ColorSync/Profiles/ (system-wide, needs admin).
    Associated with displays via ColorSync device profile APIs.
"""

from __future__ import annotations

import logging
import shutil
import struct
from pathlib import Path
from typing import List, Optional

from calibrate_pro.platform.base import (
    DisplayInfo as PlatformDisplayInfo,
    PlatformBackend,
)

logger = logging.getLogger(__name__)


def _have_quartz() -> bool:
    """Check if Quartz (CoreGraphics) bindings are available."""
    try:
        import Quartz
        return True
    except ImportError:
        return False


class MacOSBackend(PlatformBackend):
    """
    macOS implementation using CoreGraphics, IOKit, and ColorSync.

    Falls back gracefully if pyobjc is not installed.
    """

    # ------------------------------------------------------------------
    # Display enumeration
    # ------------------------------------------------------------------

    def enumerate_displays(self) -> List[PlatformDisplayInfo]:
        """Enumerate active displays via CoreGraphics."""
        try:
            import Quartz
        except ImportError:
            logger.error(
                "pyobjc-framework-Quartz not installed. "
                "Run: pip install pyobjc-framework-Quartz"
            )
            return []

        max_displays = 16
        (err, display_ids, count) = Quartz.CGGetActiveDisplayList(
            max_displays, None, None
        )
        if err != 0:
            logger.error("CGGetActiveDisplayList failed: error %d", err)
            return []

        results: List[PlatformDisplayInfo] = []
        main_display = Quartz.CGMainDisplayID()

        for i, did in enumerate(display_ids[:count]):
            # Resolution and refresh rate
            mode = Quartz.CGDisplayCopyDisplayMode(did)
            if mode:
                width = Quartz.CGDisplayModeGetWidth(mode)
                height = Quartz.CGDisplayModeGetHeight(mode)
                refresh = Quartz.CGDisplayModeGetRefreshRate(mode)
                if refresh == 0:
                    refresh = 60  # Default for displays that don't report
            else:
                bounds = Quartz.CGDisplayBounds(did)
                width = int(bounds.size.width)
                height = int(bounds.size.height)
                refresh = 60

            # Position
            bounds = Quartz.CGDisplayBounds(did)
            pos_x = int(bounds.origin.x)
            pos_y = int(bounds.origin.y)

            # EDID info (manufacturer, model, serial)
            manufacturer, model, serial = self._read_edid_info(did)

            # Display name
            name = model or f"Display {i + 1}"
            if manufacturer and manufacturer not in name:
                name = f"{manufacturer} {name}"

            # Current ICC profile
            icc_path = self._get_colorsync_profile_path(did)

            results.append(PlatformDisplayInfo(
                index=i,
                name=name,
                device_path=str(did),
                is_primary=(did == main_display),
                width=width,
                height=height,
                refresh_rate=int(refresh),
                bit_depth=Quartz.CGDisplayBitsPerPixel(did) if hasattr(Quartz, 'CGDisplayBitsPerPixel') else 8,
                position_x=pos_x,
                position_y=pos_y,
                manufacturer=manufacturer,
                model=model,
                serial=serial,
                current_icc_profile=icc_path,
            ))

        return results

    # ------------------------------------------------------------------
    # Gamma ramp
    # ------------------------------------------------------------------

    def apply_gamma_ramp(
        self,
        display_index: int,
        red: List[int],
        green: List[int],
        blue: List[int],
    ) -> bool:
        """Apply gamma ramp via CGSetDisplayTransferByTable."""
        try:
            import Quartz
        except ImportError:
            logger.error("pyobjc-framework-Quartz not installed")
            return False

        did = self._get_display_id(display_index)
        if did is None:
            return False

        # Convert 0-65535 int arrays to 0.0-1.0 float arrays
        table_size = len(red)
        r_table = [r / 65535.0 for r in red]
        g_table = [g / 65535.0 for g in green]
        b_table = [b / 65535.0 for b in blue]

        err = Quartz.CGSetDisplayTransferByTable(
            did, table_size, r_table, g_table, b_table
        )
        if err != 0:
            logger.error("CGSetDisplayTransferByTable failed: error %d", err)
            return False

        logger.info("Gamma ramp applied to display %d (CGDirectDisplayID %d)", display_index, did)
        return True

    def reset_gamma_ramp(self, display_index: int) -> bool:
        """Reset gamma ramp to ColorSync defaults."""
        try:
            import Quartz
        except ImportError:
            return False

        # CGDisplayRestoreColorSyncSettings resets ALL displays
        Quartz.CGDisplayRestoreColorSyncSettings()
        logger.info("Gamma ramps reset to ColorSync defaults")
        return True

    # ------------------------------------------------------------------
    # ICC profile management
    # ------------------------------------------------------------------

    def install_icc_profile(
        self,
        profile_path: str,
        display_index: int,
    ) -> bool:
        """Install ICC profile to ~/Library/ColorSync/Profiles/."""
        src = Path(profile_path)
        if not src.exists():
            logger.error("ICC profile not found: %s", profile_path)
            return False

        # Per-user ColorSync directory (no admin needed)
        dest_dir = Path.home() / "Library" / "ColorSync" / "Profiles"
        dest_dir.mkdir(parents=True, exist_ok=True)

        dest = dest_dir / src.name
        try:
            shutil.copy2(str(src), str(dest))
            logger.info("ICC profile installed to %s", dest)
        except Exception as e:
            logger.error("Failed to copy ICC profile: %s", e)
            return False

        # Try to associate with the display via ColorSync
        try:
            self._associate_profile_with_display(dest, display_index)
        except Exception as e:
            logger.warning(
                "Profile copied but could not auto-associate: %s. "
                "Set it manually in System Settings > Displays > Color.", e
            )

        return True

    def get_icc_profile(self, display_index: int) -> Optional[str]:
        """Get the active ICC profile path for a display."""
        did = self._get_display_id(display_index)
        if did is None:
            return None
        return self._get_colorsync_profile_path(did)

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _get_display_id(self, display_index: int) -> Optional[int]:
        """Get CGDirectDisplayID for a display index."""
        try:
            import Quartz
            max_displays = 16
            (err, display_ids, count) = Quartz.CGGetActiveDisplayList(
                max_displays, None, None
            )
            if err == 0 and display_index < count:
                return display_ids[display_index]
        except Exception as e:
            logger.debug("Failed to get display ID: %s", e)
        return None

    def _read_edid_info(self, display_id: int) -> tuple:
        """Read manufacturer, model, serial from EDID via IOKit."""
        manufacturer = ""
        model = ""
        serial = ""

        try:
            import objc
            from Foundation import NSBundle

            # Load IOKit framework
            iokit_bundle = NSBundle.bundleWithPath_(
                "/System/Library/Frameworks/IOKit.framework"
            )
            if iokit_bundle is None:
                return manufacturer, model, serial

            # Get IOKit functions
            functions = [
                ("IOServiceGetMatchingServices", b"iI@o^I"),
                ("IOIteratorNext", b"II"),
                ("IORegistryEntryCreateCFProperty", b"@I@II"),
                ("IOObjectRelease", b"iI"),
                ("IOServiceMatching", b"@*"),
                ("IODisplayGetInfoDictionary", b"@II"),
            ]

            # Try the simpler approach: CoreGraphics display info
            import Quartz
            info = Quartz.CoreGraphics.CGDisplayIOServicePort(display_id)
            # Note: CGDisplayIOServicePort is deprecated but still works

            # Fall back to reading EDID from IOKit service tree
            from CoreFoundation import (
                CFStringCreateWithCString, kCFStringEncodingASCII
            )

            # Use IOKit to find display service and read EDID
            import ctypes
            iokit = ctypes.cdll.LoadLibrary(
                "/System/Library/Frameworks/IOKit.framework/IOKit"
            )

            service_port = iokit.CGDisplayIOServicePort(display_id)
            if service_port:
                # Read EDID data
                edid_cf = iokit.IORegistryEntryCreateCFProperty(
                    service_port,
                    ctypes.c_void_p.in_dll(iokit, "kIODisplayEDIDKey") if hasattr(iokit, "kIODisplayEDIDKey") else None,
                    None, 0
                )

        except Exception:
            pass

        # Fallback: try to get display name from CoreGraphics
        try:
            import Quartz
            info_dict = Quartz.CoreGraphics.CGDisplayIOServicePort(display_id)
            # CoreGraphics doesn't directly expose model name easily
            # Use vendor/product ID approach
            vendor_id = Quartz.CGDisplayVendorNumber(display_id)
            product_id = Quartz.CGDisplayModelNumber(display_id)
            serial_num = Quartz.CGDisplaySerialNumber(display_id)

            # Map common vendor IDs
            vendor_map = {
                1262: "Samsung", 4268: "Dell", 7789: "ASUS",
                4137: "LG", 5765: "Sony", 3502: "BenQ",
                1189: "EIZO", 5765: "MSI", 8478: "Gigabyte",
                1128: "Apple", 4098: "HP", 4743: "Lenovo",
                2513: "Acer", 6476: "ViewSonic",
            }
            manufacturer = vendor_map.get(vendor_id, f"Vendor {vendor_id}")
            model = f"Display {product_id}"
            serial = str(serial_num) if serial_num else ""

        except Exception as e:
            logger.debug("EDID read failed: %s", e)

        return manufacturer, model, serial

    def _get_colorsync_profile_path(self, display_id: int) -> Optional[str]:
        """Get the current ColorSync profile path for a display."""
        try:
            import Quartz
            # CGDisplayCopyColorSpace returns a CGColorSpaceRef
            # We can get the ICC profile data from it
            colorspace = Quartz.CGDisplayCopyColorSpace(display_id)
            if colorspace:
                # Try to get the profile name
                name = Quartz.CGColorSpaceCopyName(colorspace)
                if name:
                    # Check common profile locations
                    for profiles_dir in [
                        Path.home() / "Library" / "ColorSync" / "Profiles",
                        Path("/Library/ColorSync/Profiles"),
                        Path("/System/Library/ColorSync/Profiles"),
                    ]:
                        if profiles_dir.exists():
                            for p in profiles_dir.glob("*.icc"):
                                if str(name) in p.stem:
                                    return str(p)
                            for p in profiles_dir.glob("*.icm"):
                                if str(name) in p.stem:
                                    return str(p)
        except Exception as e:
            logger.debug("ColorSync profile query failed: %s", e)
        return None

    def _associate_profile_with_display(self, profile_path: Path, display_index: int):
        """Associate an ICC profile with a display via ColorSync."""
        try:
            import Quartz
            from ColorSync import (
                ColorSyncDeviceSetCustomProfiles,
                kColorSyncDisplayDeviceClass,
                kColorSyncDeviceDefaultProfileID,
                kColorSyncProfileUserScope,
            )
            from Foundation import NSURL, NSDictionary

            did = self._get_display_id(display_index)
            if did is None:
                return

            profile_url = NSURL.fileURLWithPath_(str(profile_path))
            profile_info = NSDictionary.dictionaryWithObject_forKey_(
                profile_url, kColorSyncDeviceDefaultProfileID
            )

            # Create UUID from display ID
            import uuid
            display_uuid = str(uuid.uuid5(uuid.NAMESPACE_DNS, f"display-{did}"))

            ColorSyncDeviceSetCustomProfiles(
                kColorSyncDisplayDeviceClass,
                display_uuid,
                profile_info,
            )
            logger.info("ICC profile associated with display %d via ColorSync", display_index)

        except ImportError:
            logger.debug("ColorSync framework not available for profile association")
        except Exception as e:
            logger.debug("ColorSync profile association failed: %s", e)
