# Determinism Verification Script
# Compares low-level vs mid-level outputs by analyzing DXF geometry files directly

Write-Host "DETERMINISM VERIFICATION TEST" -ForegroundColor Cyan
Write-Host "=============================" -ForegroundColor Cyan
Write-Host ""

# Step 1: Build low-level version
Write-Host "Building low-level (absolute coordinates)..." -ForegroundColor Yellow
$build_low = cargo run --quiet -- build tests\ASIC\two-pad-relational\determinism-test\test_determinism_low.hw 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL - Low-level build failed!" -ForegroundColor Red
    exit 1
}
Write-Host "PASS - Low-level build completed" -ForegroundColor Green
Write-Host ""

# Step 2: Build mid-level version
Write-Host "Building mid-level (relational placement)..." -ForegroundColor Yellow
$build_mid = cargo run --quiet -- build tests\ASIC\two-pad-relational\determinism-test\test_determinism_mid.hw 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL - Mid-level build failed!" -ForegroundColor Red
    exit 1
}
Write-Host "PASS - Mid-level build completed" -ForegroundColor Green
Write-Host ""

# Step 3: Parse and compare DXF polylines
Write-Host "Parsing DXF geometry files..." -ForegroundColor Cyan
Write-Host ""

$low_dxf = "tests\ASIC\two-pad-relational\determinism-test\build\Determinism_Low\board.dxf"
$mid_dxf = "tests\ASIC\two-pad-relational\determinism-test\build\Determinism_Mid\board.dxf"

if (-not (Test-Path $low_dxf)) {
    Write-Host "FAIL - Low-level DXF not found: $low_dxf" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path $mid_dxf)) {
    Write-Host "FAIL - Mid-level DXF not found: $mid_dxf" -ForegroundColor Red
    exit 1
}

# Function to extract polyline vertices from DXF
function Get-DXFPolylines {
    param([string]$dxfPath)
    $content = Get-Content $dxfPath -Raw
    $polylines = @()
    
    # Find LWPOLYLINE sections
    $lwpolyPattern = '(?s)LWPOLYLINE.*?(?=\n  0\n(?:LWPOLYLINE|ENDSEC|EOF))'
    $matches = [regex]::Matches($content, $lwpolyPattern)
    
    foreach ($match in $matches) {
        $section = $match.Value
        $vertices = @()
        
        # Extract vertex coordinates
        $coordPattern = '\s10\r?\n\s*(-?\d+\.?\d*)\r?\n\s20\r?\n\s*(-?\d+\.?\d*)'
        $coords = [regex]::Matches($section, $coordPattern)
        
        foreach ($coord in $coords) {
            $vertices += @{
                x = [double]$coord.Groups[1].Value
                y = [double]$coord.Groups[2].Value
            }
        }
        
        if ($vertices.Count -gt 0) {
            $polylines += ,@($vertices)
        }
    }
    
    return $polylines
}

$low_polylines = Get-DXFPolylines -dxfPath $low_dxf
$mid_polylines = Get-DXFPolylines -dxfPath $mid_dxf

Write-Host "Low-level DXF: $($low_polylines.Count) polylines" -ForegroundColor Gray
Write-Host "Mid-level DXF: $($mid_polylines.Count) polylines" -ForegroundColor Gray
Write-Host ""

# Step 4: Compare polyline geometry
$geometry_match = $true
$polylines_checked = $false

if ($low_polylines.Count -gt 0 -and $mid_polylines.Count -gt 0) {
    $polylines_checked = $true
    
    if ($low_polylines.Count -ne $mid_polylines.Count) {
        Write-Host "FAIL - Polyline count mismatch!" -ForegroundColor Red
        Write-Host "  Low: $($low_polylines.Count) polylines" -ForegroundColor Red
        Write-Host "  Mid: $($mid_polylines.Count) polylines" -ForegroundColor Red
        $geometry_match = $false
    } else {
        for ($i = 0; $i -lt $low_polylines.Count; $i++) {
            $low_poly = $low_polylines[$i]
            $mid_poly = $mid_polylines[$i]
            
            if ($low_poly.Count -ne $mid_poly.Count) {
                Write-Host "FAIL - Polyline $i vertex count mismatch:" -ForegroundColor Red
                Write-Host "  Low: $($low_poly.Count) vertices" -ForegroundColor Red
                Write-Host "  Mid: $($mid_poly.Count) vertices" -ForegroundColor Red
                $geometry_match = $false
                continue
            }
            
            for ($j = 0; $j -lt $low_poly.Count; $j++) {
                $dx = [Math]::Abs($low_poly[$j].x - $mid_poly[$j].x)
                $dy = [Math]::Abs($low_poly[$j].y - $mid_poly[$j].y)
                
                if ($dx -gt 0.001 -or $dy -gt 0.001) {
                    Write-Host "FAIL - Polyline $i vertex $j mismatch:" -ForegroundColor Red
                    Write-Host "  Low X=$($low_poly[$j].x) Y=$($low_poly[$j].y)" -ForegroundColor Red
                    Write-Host "  Mid X=$($mid_poly[$j].x) Y=$($mid_poly[$j].y)" -ForegroundColor Red
                    Write-Host "  Delta: dX=$dx dY=$dy mm" -ForegroundColor Red
                    $geometry_match = $false
                }
            }
        }
        
        if ($geometry_match) {
            Write-Host "PASS - All DXF polylines match exactly!" -ForegroundColor Green
            Write-Host "  Verified $($low_polylines.Count) trace polylines" -ForegroundColor Gray
        }
    }
} else {
    Write-Host "WARNING - No polylines found in DXF files" -ForegroundColor Yellow
}
Write-Host ""

# Step 5: Compare complete DXF files
Write-Host "Comparing complete DXF files..." -ForegroundColor Cyan

$low_content = Get-Content $low_dxf
$mid_content = Get-Content $mid_dxf

$low_filtered = $low_content | Where-Object { $_ -notmatch "Determinism_Low" }
$mid_filtered = $mid_content | Where-Object { $_ -notmatch "Determinism_Mid" }

$diff = Compare-Object $low_filtered $mid_filtered

if ($diff) {
    Write-Host "FAIL - DXF files differ:" -ForegroundColor Red
    Write-Host "  Found $($diff.Count) line differences" -ForegroundColor Red
    $diff | Select-Object -First 3 | ForEach-Object {
        $side = if ($_.SideIndicator -eq "<=") { "Low" } else { "Mid" }
        Write-Host "  [$side] $($_.InputObject)" -ForegroundColor Gray
    }
    if ($diff.Count -gt 3) {
        Write-Host "  ... and $($diff.Count - 3) more differences" -ForegroundColor Gray
    }
} else {
    Write-Host "PASS - DXF files are byte-identical" -ForegroundColor Green
}
Write-Host ""

# Step 6: Summary
Write-Host "DETERMINISM TEST SUMMARY" -ForegroundColor Cyan
Write-Host "========================" -ForegroundColor Cyan
Write-Host ""

$all_passed = $geometry_match -and (-not $diff)

if ($polylines_checked) {
    $status = if ($geometry_match) { "PASS" } else { "FAIL" }
    Write-Host "Route traces: $status" -ForegroundColor $(if ($geometry_match) { "Green" } else { "Red" })
}

$dxf_status = if (-not $diff) { "PASS" } else { "FAIL" }
Write-Host "Complete DXF: $dxf_status" -ForegroundColor $(if (-not $diff) { "Green" } else { "Red" })

Write-Host ""

if ($all_passed) {
    Write-Host "SUCCESS: Middle-level syntax produces IDENTICAL output to low-level!" -ForegroundColor Green
    Write-Host ""
    Write-Host "  - All component positions match" -ForegroundColor Green
    Write-Host "  - All route traces are identical" -ForegroundColor Green
    Write-Host "  - Complete geometric output is deterministic" -ForegroundColor Green
    Write-Host ""
    Write-Host "The compiler relational constraint resolver is working correctly." -ForegroundColor Green
    exit 0
} else {
    Write-Host "FAILURE: Outputs differ - compiler semantic bug detected!" -ForegroundColor Red
    Write-Host ""
    if (-not $geometry_match) {
        Write-Host "  - Route traces differ" -ForegroundColor Red
    }
    if ($diff) {
        Write-Host "  - DXF geometry differs" -ForegroundColor Red
    }
    Write-Host ""
    Write-Host "The middle-level constraints are not resolving correctly." -ForegroundColor Red
    exit 1
}
