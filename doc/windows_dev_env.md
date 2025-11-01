## Install Package Manager (Chocolatey or Winget)

### Option 1: Chocolatey (Recommended)

Open PowerShell as Administrator and run:

```powershell
Set-ExecutionPolicy Bypass -Scope Process -Force; [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
```

### Option 2: Winget (Built-in Windows Package Manager)

Winget comes pre-installed on Windows 10 (version 1809+) and Windows 11. If not available, install from Microsoft Store: "App Installer"

## Install Git

### Using Chocolatey:
```powershell
choco install git -y
```

### Using Winget:
```powershell
winget install Git.Git
```

### Manual Installation:
Download from [https://git-scm.com/download/win](https://git-scm.com/download/win)

Configure Git with your credentials:

```bash
git config --global user.name "Your Name"
git config --global user.email "your.email@example.com"
```

## Install Visual Studio Build Tools

Flutter on Windows requires Visual Studio Build Tools for native compilation.

### Using Chocolatey:
```powershell
choco install visualstudio2022buildtools -y
choco install visualstudio2022-workload-vctools -y
```

### Using Winget:
```powershell
winget install Microsoft.VisualStudio.2022.BuildTools
```

### Manual Installation:
Download Visual Studio Installer from [https://visualstudio.microsoft.com/downloads/](https://visualstudio.microsoft.com/downloads/) and install "Build Tools for Visual Studio 2022" with C++ build tools workload.

## Install Flutter

### Using Chocolatey:
```powershell
choco install flutter -y
```

### Using Winget:
```powershell
winget install Google.Flutter
```

### Manual Installation:
1. Download Flutter SDK from [https://docs.flutter.dev/get-started/install/windows](https://docs.flutter.dev/get-started/install/windows)
2. Extract to `C:\flutter` (or your preferred location)
3. Add `C:\flutter\bin` to your PATH environment variable

Verify installation:
```powershell
flutter doctor
```

## Install Rust

```powershell
# Download and run rustup installer
# Visit https://rustup.rs/ and download rustup-init.exe
# Or use chocolatey/winget:

# Using Chocolatey:
choco install rust -y

# Using Winget:
winget install Rustlang.Rustup
```

After installation, restart your terminal and verify:
```powershell
rustc --version
cargo --version
```

## Clone the AtomCAD Repository

```powershell
# Navigate to where you want the project
cd C:\Users\%USERNAME%\Documents  # or wherever you prefer

# Clone the repository
git clone https://github.com/yourusername/flutter_cad.git  # Replace with actual repo URL
cd flutter_cad
```

## Install Flutter Dependencies

```powershell
# Get Flutter dependencies
flutter pub get

# Check if everything is set up correctly
flutter doctor
```

## Install Additional Tools (if needed)

If `flutter doctor` shows any issues, you might need:

```powershell
# Windows SDK (usually comes with Visual Studio Build Tools)
# If needed separately:
choco install windows-sdk-10-version-2004-all -y

# Android SDK (if developing for Android)
choco install android-sdk -y
```

## Build and Test

```powershell
# Build the Rust library
cd rust
cargo build

# Go back to project root and test Flutter
cd ..
flutter run
```

## Additional Windows-Specific Notes

- **PowerShell Execution Policy**: If you encounter execution policy errors, run PowerShell as Administrator and execute:
  ```powershell
  Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
  ```

- **Windows Defender**: You may need to add exclusions for your development folders to improve build performance:
  - Add `C:\flutter` to Windows Defender exclusions
  - Add your project directory to exclusions
  - Add `%USERPROFILE%\.cargo` to exclusions

- **Long Path Support**: Enable long path support in Windows if you encounter path length issues:
  ```powershell
  # Run as Administrator
  New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
  ```

- **Environment Variables**: After installing tools, you may need to restart your terminal or add paths manually to your system PATH if automatic installation doesn't work.