@echo off
REM Update atomCAD architecture diagram
REM This script runs the LOC counter and SVG generator

echo ========================================
echo  atomCAD Architecture Diagram Generator
echo ========================================
echo.

echo Step 1/2: Counting lines of code...
python count_loc.py
if errorlevel 1 (
    echo Error running count_loc.py
    pause
    exit /b 1
)

echo.
echo Step 2/2: Generating SVG diagram...
python generate_architecture_diagram.py
if errorlevel 1 (
    echo Error running generate_architecture_diagram.py
    pause
    exit /b 1
)

echo.
echo ========================================
echo  Success! Diagram updated.
echo  Location: doc\architecture_diagram.svg
echo ========================================
pause
