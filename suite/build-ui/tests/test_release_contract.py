from __future__ import annotations

import ast
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]


def _subprocess_argv(text: str, method: str) -> list[str]:
    match = re.search(r'python -c "(?P<source>[^"]+)"', text, re.DOTALL)
    assert match is not None, "CI must invoke the type checker through python -c"
    source = " ".join(line.strip() for line in match.group("source").splitlines())
    tree = ast.parse(source)
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        if not (
            isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name)
            and node.func.value.id == "subprocess"
            and node.func.attr == method
        ):
            continue
        assert node.args and isinstance(node.args[0], ast.List)
        argv: list[str] = []
        for item in node.args[0].elts:
            if (
                isinstance(item, ast.Attribute)
                and isinstance(item.value, ast.Name)
                and item.value.id == "sys"
                and item.attr == "executable"
            ):
                argv.append("sys.executable")
            elif isinstance(item, ast.Constant) and isinstance(item.value, str):
                argv.append(item.value)
            elif isinstance(item, ast.Starred) and isinstance(item.value, ast.Name):
                argv.append(f"*{item.value.id}")
            else:
                raise AssertionError(ast.dump(item))
        return argv
    raise AssertionError(f"subprocess.{method} call not found")


def test_ci_runs_both_qt_binding_lanes() -> None:
    text = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    assert "pyqt6" in text
    assert "pyside6" in text
    assert "QT_API" in text
    assert "QT_QPA_PLATFORM" in text
    assert "tests/qt_binding_probe.py" in text
    assert "tests/qt_selection_mismatch_probe.py" in text
    assert "--cov-fail-under=70" in text
    assert _subprocess_argv(text, "check_output") == [
        "sys.executable",
        "-m",
        "qtpy",
        "mypy-args",
    ]
    assert _subprocess_argv(text, "call") == [
        "sys.executable",
        "-m",
        "mypy",
        "*flags",
        "build_ui",
    ]
    assert "--expect-no-binding" in text


def test_release_verifies_built_wheel_with_both_extras_before_trusted_publish() -> None:
    text = (ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
    assert "actions/checkout@v6" in text
    assert "actions/checkout@v7" not in text
    assert "actions/setup-python@v6" in text
    assert "actions/upload-artifact@v7" in text
    assert "scripts/verify_wheel.py" in text
    assert "pyqt6" in text
    assert "pyside6" in text
    assert "--expect-no-binding" in text
    assert 'cd "$RUNNER_TEMP"' in text
    assert "--source-root" in text
    assert text.count("pip check") >= 3
    assert "packaging>=24,<26" in text
    assert "tests/qt_selection_mismatch_probe.py" in text
    assert "release:" in text
    assert "types: [published]" in text
    assert "pypa/gh-action-pypi-publish@release/v1" in text
    assert "id-token: write" in text
    assert "\n  publish:" in text
    assert "needs: candidate" in text
    assert "github.event_name == 'release'" in text
    assert "inputs.publish" in text
    assert "type: boolean" in text
    assert "release_tag" in text
    assert "environment: pypi" in text
    assert 'gh release download "$RELEASE_TAG" --dir dist' in text
    assert '--repo "$GITHUB_REPOSITORY"' in text
    assert "sha256sum --check SHA256SUMS.txt" in text
    assert "python -m twine check dist/*.whl dist/*.tar.gz" in text
    assert "rm -f dist/SHA256SUMS.txt" in text
    publish_section = text.split("\n  publish:", 1)[1]
    assert "actions/setup-python@v6" in publish_section
    assert "python -m pip install --upgrade twine packaging" in publish_section
    release_contract_command = "python -m pytest tests/test_release_contract.py -q"
    assert release_contract_command in text
    assert text.index(release_contract_command) < text.index("actions/upload-artifact@v7")


def test_wheel_verifier_exists() -> None:
    assert (ROOT / "scripts" / "verify_wheel.py").is_file()
    assert (ROOT / "tests" / "qt_selection_mismatch_probe.py").is_file()


def test_docs_publish_explicit_binding_install_contract() -> None:
    texts = "\n".join(
        (ROOT / name).read_text(encoding="utf-8")
        for name in (
            "README.md",
            "USAGE.md",
            "MIGRATING.md",
            "ARCHITECTURE.md",
            "SECURITY.md",
            "CONTRIBUTING.md",
            "AGENTS.md",
            "THIRD_PARTY_NOTICES.md",
        )
    )
    assert "build-ui[pyside6]" in texts
    assert "build-ui[pyqt6]" in texts
    assert "QT_API" in texts
    assert "QtPy" in texts
    assert "PyQt6 only" not in texts
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    usage = (ROOT / "USAGE.md").read_text(encoding="utf-8")
    for public_doc in (readme, usage):
        assert "build-ui[pyside6]" in public_doc
        assert "build-ui[pyqt6]" in public_doc
        assert "installs QtPy but no Qt binding" in public_doc
    notice = (ROOT / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
    for token in ("QtPy", "MIT", "PyQt6", "GPL", "PySide6", "LGPLv3"):
        assert token in notice
    assert "does not relicense" in notice
    assert "PyQt6 only" not in (ROOT / "docs" / "ENTERPRISE-READINESS.md").read_text(encoding="utf-8")
    # The README hero loads from an absolute URL on main, so it also renders on PyPI (4 October 2026).
    assert ('src="https://raw.githubusercontent.com/HarperZ9/build-ui/main/docs/art/hero-light.svg"') in readme
    assert 'src="docs/art/hero-light.svg"' not in readme
    for hero in ("hero-light.svg", "hero-dark.svg"):
        assert "PyQt6 theme" not in (ROOT / "docs" / "art" / hero).read_text(encoding="utf-8")
    assert not (ROOT / "docs" / "brand" / "build-ui-hero.png").exists()


def test_security_policy_supports_the_released_2_x_line() -> None:
    security = (ROOT / "SECURITY.md").read_text(encoding="utf-8")
    normalized = " ".join(security.split())
    assert "latest release on the supported 2.x line" in normalized
    assert "Until a 2.0 line exists" not in security


def test_contributor_verification_runs_both_binding_isolated_probes() -> None:
    contributing = (ROOT / "CONTRIBUTING.md").read_text(encoding="utf-8")
    for token in (
        ".venv-pyqt6",
        ".venv-pyside6",
        'QT_API = "pyqt6"',
        'QT_API = "pyside6"',
        "tests/qt_binding_probe.py",
        "tests/qt_selection_mismatch_probe.py --installed-api pyqt6",
        "tests/qt_selection_mismatch_probe.py --installed-api pyside6",
    ):
        assert token in contributing


def test_migration_docs_record_the_accepted_calibrate_candidate_handoff() -> None:
    migrating = (ROOT / "MIGRATING.md").read_text(encoding="utf-8")
    for token in (
        "dist/build_ui-2.0.0-py3-none-any.whl",
        "dist/SHA256SUMS.txt",
        "BUILD_UI_2_WHEEL",
        "Get-FileHash -LiteralPath $env:BUILD_UI_2_WHEEL -Algorithm SHA256",
        'python -m pip install "$($env:BUILD_UI_2_WHEEL)[pyside6]"',
        "python -m pytest tests/test_qt_binding_contract.py -q",
        "91066089a8a7468b3249388a7508bb4eed42142269cca263927fa3a83e1884ff",
        "build-ui-2-pyside-proof=pass",
    ):
        assert token in migrating
