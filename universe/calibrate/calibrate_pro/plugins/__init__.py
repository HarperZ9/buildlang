"""
Calibrate Pro Plugin Architecture.

Provides a plugin system for extending Calibrate Pro with custom LUT
generators, device drivers, output format exporters, and panel data
sources.

Plugins are Python files with a ``register(manager)`` function and an
optional ``PLUGIN_INFO`` dict. They are discovered from:

- ``~/.calibrate-pro/plugins/``
- A ``plugins/`` directory next to the Calibrate Pro package
- Python entry points under the ``calibrate_pro.plugins`` group
"""

from calibrate_pro.plugins.manager import PluginManager, PluginInfo

__all__ = ["PluginManager", "PluginInfo"]
