@echo off
REM Build script for hwc compiler (Windows)

echo Building HWC Compiler...
echo.

echo [1/3] Clippy - Rust code quality check...
cargo clippy --all-targets --all-features -- -D warnings

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo Clippy failed! Fix Rust code issues before continuing.
    exit /b 1
)

echo.
echo [2/3] Building release compiler...
cargo build --release --quiet
if %ERRORLEVEL% NEQ 0 (
    echo Failed to build compiler executable
    exit /b 1
)

echo.
echo [3/3] Testing HWC language compiler...
echo.
echo   Running: stress_test.hw (comprehensive baseline)
target\release\hwc.exe build examples\stress_test.hw -o stress_output
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo   ✗ Stress test failed - compiler regression detected
    exit /b 1
)
echo   ✓ Stress test passed
echo.

echo ========================================
echo All checks passed!
echo ========================================
echo.
echo Compiler: target\release\hwc.exe
echo Test outputs: stress_output\
echo   - board.gtl (Gerber for PCB manufacturing)
echo   - board.obj (3D model)
echo   - sim.py (Blender script)
