"""
Plugin Manager for Calibrate Pro.

Discovers, loads, and manages plugins that extend the calibration
pipeline with custom LUT generators, device drivers, output format
exporters, and panel data sources.

Plugin convention
-----------------
Each plugin is a Python file (``.py``) with a top-level
``register(manager)`` function and an optional ``PLUGIN_INFO`` dict::

    # example_plugin.py
    PLUGIN_INFO = {
        "name": "My Plugin",
        "version": "1.0",
        "author": "Author",
        "description": "Custom LUT generator",
        "plugin_type": "lut_generator",
    }

    def register(manager):
        manager.register_lut_generator("my_custom_lut", my_lut_function)
        manager.register_output_format("my_format", my_export_function)
"""

import os
import sys
import importlib
import importlib.util
import logging
from pathlib import Path
from typing import Callable, Dict, List, Optional
from dataclasses import dataclass, field

from calibrate_pro import __version__

logger = logging.getLogger(__name__)


@dataclass
class PluginInfo:
    """Metadata describing a discovered plugin."""
    name: str
    version: str
    author: str
    description: str
    plugin_type: str  # "lut_generator", "device_driver", "output_format", "panel_source"
    file_path: Optional[str] = None
    loaded: bool = False
    error: Optional[str] = None


class PluginManager:
    """
    Discover, load, and manage Calibrate Pro plugins.

    Args:
        plugin_dirs: List of directories to scan for plugins.
                     Defaults to ``[~/.calibrate-pro/plugins/, ./plugins/]``.
    """

    def __init__(self, plugin_dirs: Optional[List[str]] = None):
        if plugin_dirs is not None:
            self._plugin_dirs = [Path(d) for d in plugin_dirs]
        else:
            self._plugin_dirs = self._default_dirs()

        self._plugins: Dict[str, PluginInfo] = {}
        self._lut_generators: Dict[str, Callable] = {}
        self._output_formats: Dict[str, Callable] = {}
        self._device_drivers: Dict[str, Callable] = {}
        self._panel_sources: Dict[str, Callable] = {}

    # ------------------------------------------------------------------
    # Default directories
    # ------------------------------------------------------------------

    @staticmethod
    def _default_dirs() -> List[Path]:
        """Return the default list of plugin directories."""
        dirs: List[Path] = []

        # User plugin directory
        home = Path.home()
        user_dir = home / ".calibrate-pro" / "plugins"
        dirs.append(user_dir)

        # Local plugins/ next to the calibrate_pro package
        pkg_dir = Path(__file__).resolve().parent.parent
        local_dir = pkg_dir / "plugins"
        dirs.append(local_dir)

        return dirs

    # ------------------------------------------------------------------
    # Registration API (called by plugins inside their register())
    # ------------------------------------------------------------------

    def register_lut_generator(self, name: str, func: Callable) -> None:
        """Register a custom LUT generator function."""
        self._lut_generators[name] = func
        logger.debug("Registered LUT generator: %s", name)

    def register_output_format(self, name: str, func: Callable) -> None:
        """Register a custom output format exporter."""
        self._output_formats[name] = func
        logger.debug("Registered output format: %s", name)

    def register_device_driver(self, name: str, func: Callable) -> None:
        """Register a custom device driver."""
        self._device_drivers[name] = func
        logger.debug("Registered device driver: %s", name)

    def register_panel_source(self, name: str, func: Callable) -> None:
        """Register a custom panel data source."""
        self._panel_sources[name] = func
        logger.debug("Registered panel source: %s", name)

    # ------------------------------------------------------------------
    # Query API
    # ------------------------------------------------------------------

    def get_lut_generators(self) -> Dict[str, Callable]:
        """Get all registered custom LUT generators."""
        return dict(self._lut_generators)

    def get_output_formats(self) -> Dict[str, Callable]:
        """Get all registered custom output format exporters."""
        return dict(self._output_formats)

    def get_device_drivers(self) -> Dict[str, Callable]:
        """Get all registered custom device drivers."""
        return dict(self._device_drivers)

    def get_panel_sources(self) -> Dict[str, Callable]:
        """Get all registered custom panel data sources."""
        return dict(self._panel_sources)

    def get_discovered_plugins(self) -> List[PluginInfo]:
        """Return metadata for all discovered plugins (loaded or not)."""
        return list(self._plugins.values())

    # ------------------------------------------------------------------
    # Discovery
    # ------------------------------------------------------------------

    def discover_plugins(self) -> List[PluginInfo]:
        """
        Scan plugin directories and entry_points for plugins.

        Returns a list of PluginInfo for every plugin found. Plugins
        are *not* loaded automatically -- call :meth:`load_plugin` to
        activate one.
        """
        self._plugins.clear()

        # 1. Scan filesystem directories
        for plugin_dir in self._plugin_dirs:
            if not plugin_dir.is_dir():
                continue
            for py_file in sorted(plugin_dir.glob("*.py")):
                if py_file.name.startswith("_"):
                    continue
                info = self._inspect_file(py_file)
                if info is not None:
                    self._plugins[info.name] = info

        # 2. Scan setuptools entry_points (group = "calibrate_pro.plugins")
        self._discover_entry_points()

        return list(self._plugins.values())

    def _inspect_file(self, py_file: Path) -> Optional[PluginInfo]:
        """Read PLUGIN_INFO from a .py file without executing it."""
        try:
            import ast

            source = py_file.read_text(encoding="utf-8")
            tree = ast.parse(source, filename=str(py_file))

            plugin_info_dict: Optional[dict] = None
            has_register = False

            for node in ast.iter_child_nodes(tree):
                # Look for PLUGIN_INFO = { ... }
                if isinstance(node, ast.Assign):
                    for target in node.targets:
                        if isinstance(target, ast.Name) and target.id == "PLUGIN_INFO":
                            # Safely evaluate the dict literal
                            try:
                                plugin_info_dict = ast.literal_eval(node.value)
                            except (ValueError, TypeError):
                                pass

                # Look for def register(...)
                if isinstance(node, ast.FunctionDef) and node.name == "register":
                    has_register = True

            if not has_register:
                return None

            name = py_file.stem
            version = "0.0"
            author = "Unknown"
            description = ""
            plugin_type = "unknown"

            if plugin_info_dict and isinstance(plugin_info_dict, dict):
                name = plugin_info_dict.get("name", name)
                version = plugin_info_dict.get("version", version)
                author = plugin_info_dict.get("author", author)
                description = plugin_info_dict.get("description", description)
                plugin_type = plugin_info_dict.get("plugin_type", plugin_type)

            return PluginInfo(
                name=name,
                version=version,
                author=author,
                description=description,
                plugin_type=plugin_type,
                file_path=str(py_file),
                loaded=False,
            )

        except Exception as exc:
            logger.warning("Could not inspect plugin %s: %s", py_file, exc)
            return PluginInfo(
                name=py_file.stem,
                version="?",
                author="?",
                description="(failed to inspect)",
                plugin_type="unknown",
                file_path=str(py_file),
                loaded=False,
                error=str(exc),
            )

    def _discover_entry_points(self) -> None:
        """Discover plugins registered via setuptools entry_points."""
        try:
            if sys.version_info >= (3, 10):
                from importlib.metadata import entry_points
                eps = entry_points(group="calibrate_pro.plugins")
            else:
                from importlib.metadata import entry_points as _all_eps
                all_eps = _all_eps()
                eps = all_eps.get("calibrate_pro.plugins", [])

            for ep in eps:
                info = PluginInfo(
                    name=ep.name,
                    version="0.0",
                    author="(entry_point)",
                    description=f"Entry point: {ep.value}",
                    plugin_type="unknown",
                    file_path=None,
                    loaded=False,
                )
                if info.name not in self._plugins:
                    self._plugins[info.name] = info

        except Exception as exc:
            logger.debug("entry_points discovery failed: %s", exc)

    # ------------------------------------------------------------------
    # Loading
    # ------------------------------------------------------------------

    def load_plugin(self, name: str) -> bool:
        """
        Load a specific plugin by name.

        The plugin's ``register(manager)`` function is called with this
        manager instance so that the plugin can register its extensions.

        Returns True on success, False on failure.
        """
        info = self._plugins.get(name)
        if info is None:
            logger.error("Plugin %r not found", name)
            return False

        if info.loaded:
            return True

        try:
            if info.file_path:
                module = self._load_file(info.file_path, name)
            else:
                # Entry point -- import by module name
                module = importlib.import_module(name)

            register_fn = getattr(module, "register", None)
            if register_fn is None:
                info.error = "No register() function found"
                logger.error("Plugin %r has no register() function", name)
                return False

            register_fn(self)
            info.loaded = True

            # Update info from PLUGIN_INFO if present
            pi = getattr(module, "PLUGIN_INFO", None)
            if isinstance(pi, dict):
                info.version = pi.get("version", info.version)
                info.author = pi.get("author", info.author)
                info.description = pi.get("description", info.description)
                info.plugin_type = pi.get("plugin_type", info.plugin_type)

            logger.info("Loaded plugin: %s v%s", info.name, info.version)
            return True

        except Exception as exc:
            info.error = str(exc)
            logger.error("Failed to load plugin %r: %s", name, exc)
            return False

    def load_all(self) -> int:
        """
        Load all discovered plugins.

        Returns the number of successfully loaded plugins.
        """
        loaded = 0
        for name in list(self._plugins):
            if self.load_plugin(name):
                loaded += 1
        return loaded

    @staticmethod
    def _load_file(file_path: str, module_name: str):
        """Import a .py file as a module."""
        spec = importlib.util.spec_from_file_location(
            f"calibrate_pro_plugin_{module_name}",
            file_path,
        )
        if spec is None or spec.loader is None:
            raise ImportError(f"Cannot create module spec for {file_path}")

        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module


# ---------------------------------------------------------------------------
# CLI helper
# ---------------------------------------------------------------------------

def print_discovered_plugins(plugin_dirs: Optional[List[str]] = None) -> None:
    """
    Print discovered plugins to stdout.

    Used by the ``calibrate-pro plugins`` CLI command.
    """
    print(f"\nCalibrate Pro v{__version__} - Plugins")
    print("=" * 56)

    mgr = PluginManager(plugin_dirs=plugin_dirs)
    plugins = mgr.discover_plugins()

    dirs_display = [str(d) for d in mgr._plugin_dirs]

    print(f"\n  Plugin directories:")
    for d in dirs_display:
        exists_tag = "" if Path(d).is_dir() else " (not found)"
        print(f"    {d}{exists_tag}")

    print(f"\n  Discovered plugins:")

    if not plugins:
        print(f"    (none - place .py files in {dirs_display[0]})")
    else:
        for p in plugins:
            status = ""
            if p.error:
                status = f" [ERROR: {p.error}]"
            elif p.loaded:
                status = " [loaded]"
            print(f"    {p.name} v{p.version} ({p.plugin_type}) - {p.description}{status}")
            if p.file_path:
                print(f"      File: {p.file_path}")

    print()
