# PowerShell script to build and push wasix_eth windows node to a Docker registry

param (
    [Parameter(Mandatory=$true)]
    [string]$Registry,

    [string]$Tag = "latest"
)

function Build-Push-Node {
    param([string]$Type)
    $NodeBin = Join-Path $PSScriptRoot "../../wasix_eth/target/release/wasix_eth.exe"
    Write-Host "Checking for $Type node binary at $NodeBin..." -ForegroundColor Cyan
    if (-not (Test-Path $NodeBin)) {
        Write-Host "ERROR: $Type node binary not found. Please build it first: cd wasix_eth; cross build --release --target x86_64-pc-windows-msvc" -ForegroundColor Red
        return
    }

    $FullTag = "${Registry}/wasix-eth-${Type}:${Tag}"
    Write-Host "Building image $FullTag..." -ForegroundColor Cyan
    $NodeDir = Join-Path $PSScriptRoot "../../wasix_eth"
    docker build -t $FullTag -f "$NodeDir/Dockerfile.$Type" "$NodeDir"

    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to build $FullTag. Skipping push." -ForegroundColor Red
        return
    }

    Write-Host "Pushing $FullTag..." -ForegroundColor Cyan
    docker push $FullTag
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to push $FullTag. Ensure you are logged in (docker login) and have permission." -ForegroundColor Red

        # Diagnostic help
        if ($Registry -notlike "*.*" -and $Registry -notlike "*/*") {
            Write-Host "ERROR: Registry '$Registry' seems to be missing a hostname." -ForegroundColor Red
            Write-Host "TIP: If you are using Docker Hub, use 'docker.io/$Registry' as your registry." -ForegroundColor Yellow
        }

        if ($Registry -like "ghcr.io*") {
            Write-Host "TIP: For GitHub Container Registry (ghcr.io), ensure your Personal Access Token (PAT) has the 'write:packages' scope." -ForegroundColor Yellow
        } elseif ($Registry -like "*azurecr.io*") {
            Write-Host "TIP: For Azure Container Registry, ensure you have the 'AcrPush' role or equivalent permissions." -ForegroundColor Yellow
        }
    }
}

Build-Push-Node -Type "windows"

Write-Host "Process completed for $Registry with tag $Tag" -ForegroundColor Green
