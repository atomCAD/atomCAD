## How should atomCAD do energy minimization?

We can do energy minimization with OpenMM (https://openmm.org/).

OpenMM has a low level C++ API and a high level Python API. Force field xml file are only supported by the Python API, so we need to use the Python API unless we want to replicate a lot of the Python API code in Rust or C++.

To do an energy minimization with the OpenMM Python API we need to supply the molecule
and a force field. The force field is an xml file.
OpenMM force field files are in the FFXML format.
In these files atom types are bound to residue names + atom names in the molecule (residue names and atom names are present in molecule file formats like pdb)
The forces are then defined in terms of atom types.
As far as I understand this means that we canot use these traditional FFXML files unless
we produce molecules in our applications with atoms in the supported residue names + atom names of the force fields.

If we use the OpenFF toolkit we can use OpenFF force field files which are in a different format (SMIRNOFF format). Particularly, the whole atom name and atom type business is no longer present and forces are defined in terms of SMIRKS patterns (which is an extension of SMARTS which is an extension of SMILES) which do not depend on how we name atoms in in the molecule, so in this sense these files are more generic.

The latest OpenFF force field file is version 2.2.1
https://github.com/openforcefield/openff-forcefields/blob/main/openforcefields/offxml/openff-2.2.1.offxml

I studies this force field and it is for organic chemistry. It supports lots of configurations including carbon, hidrogen, nitrogen, oxygen, sulfur and maybe a few more elements but it is by far not generic. It does not support Silicon at all.

To have a fallback when OpenFF is not applicable we need to use a more generic force field. The only force field I know of that is generic enough to contain all elements in the periodic table up to atomic number more than 100 is UFF.

UFF is obviously a lower quality force field than OpenFF.

We can approach it from Python too.
UFF is not available as any kind of xml file, it can be used from Python the following ways:

- 1. Use the RdKit UFF feature to directly do the UFF energy minimization
- 2. Use the RdKit to create a force field for our molecule and transfer this molecule into OpenMM and do the minimization there (supposed to be more performant)
Ways to do UFF 
- 3. Use Openbabel UFF feature to do the energy minimization. Openbabel's UFF implementation is not so strictly only what was written in the original UFF paper in 1992, so it is more forgiving, more broadly applicable, so even more generic.
- 4. Use Openbabel UFF feature to create a force field for our molecule and transfer this OpenMM and do the minimization there (supposed to be more performant). This is more involved than transfering from RdKit, as there is no direct support and SDF files need to be used as an intermediary.

We will support both RDKit approaches (approach 1 and 2). Later we might also support the Openbabel approaches.

## How does MSep do energy minimization?

MSep uses OpenMM and OpenFF 2.1.0 to do energy minimization as we described above.
As a generic fallback they do not use UFF but they have an additional force field file
which they call MSep One extension 0.0.1

This is the OpenFF force field they use:

https://github.com/MSEP-one/msep.one/blob/main/godot_project/python/scripts/offxml/openff-2.1.0.offxml

And this is the MSep One extension they use:

https://github.com/MSEP-one/msep.one/blob/main/godot_project/python/scripts/offxml_extensions/msep.one_extension-0.0.1.offxml

The Msep One extension contains mostly Silicon approximations and approximations for some other elements like chlorine. It is not a generic force field like UFF.

As MSEP One is MIT licensed I think we can use this file in our application.

## Plan summary

Our current planned approach:
- Use OpenMM with the merge of the OpenFF 2.2.1 force field and the MSep One extension 0.0.1 to do energy minimization if it supports our molecule.
- Use UFF as a fallback as described above.
- Find out whether instead of using as a fallback whether UFF can be merged with the OpenFF 2.2.1 force field too.

MSep is written in GodotScript so they cannot easily integrate Python. They launch a server process written in Python (they call it openmm server) and communicattion between the Python code and GodotScript code is using sockets.

https://github.com/MSEP-one/msep.one/blob/main/godot_project/python/scripts/openmm_server.py

Fortunately there is a simpler way to call Python libraries from Rust: by using the pyo3 crate, so we will use this.

Pyo3 allows embedding a Python interpreter directly within a Rust application. It can be used to:

- Load Python modules (like rdkit, openmm, openff).
- Pass data from Rust to Python, call Python functions with that data and get the results back into the Rust code.

## Python environment

Currently we use the python environment of the user as is. Later we will need to set up a self-contained installation approach for atomCAD. Until then I document here what needs to be installed on the user's computer. Some of this is windows specific.

We need Python 3.11, openmm and openff-toolkit

installation docs:

https://docs.openmm.org/latest/userguide/application/01_getting_started.html#installing-openmm

https://docs.openforcefield.org/projects/toolkit/en/stable/installation.html

Please note that pip install is not available for openmm-forcefield.
Recommended to install is through mamba, which is a conda drop-in replacement.

## Linux installation summary

This part of the document describes a minimal, reproducible Python environment for running the Python parts of the atomCAD project (OpenMM and OpenFF Toolkit). It assumes WSL2 or a native Linux (Ubuntu/Debian) environment. This part is AI generated.

---

### 1. Install Miniconda (or Mambaforge)

Use Miniconda for a simple install, or Mambaforge if you prefer faster dependency solving.

```bash
# Download Miniconda (or replace URL with Mambaforge if you prefer)
wget https://repo.anaconda.com/miniconda/Miniconda3-latest-Linux-x86_64.sh

# Run installer and follow prompts
bash Miniconda3-latest-Linux-x86_64.sh

# Reload shell so `conda` is available (or open a new terminal)
source ~/.bashrc
```

---

### 2. Create a dedicated environment

Prefer creating the environment from `conda-forge` to ensure binary compatibility.


```bash
mamba create -n atomcad -c conda-forge python=3.11 libgcc-ng libstdcxx-ng openmm openff-toolkit-base rdkit openff-interchange-base -y
mamba activate atomcad
```

> Installing both `openmm` and `openff-toolkit` from `conda-forge` keeps native libraries and runtimes consistent.

---

### 3. Fix common C++ runtime issues

If Python raises an error about `GLIBCXX` or missing `libstdc++.so.6` symbols, install/update the runtime packages from `conda-forge` inside the environment:

```bash
conda activate atomcad
conda install -c conda-forge libgcc-ng libstdcxx-ng -y
# Optionally force-reinstall openmm to ensure consistent deps
conda install -c conda-forge openmm --update-deps --force-reinstall -y
```

---

---

### 4. Verify the installation

Run small import checks to ensure OpenMM/OpenFF load correctly and that the environment provides the expected libraries.

```bash
conda activate atomcad
python -c "import simtk.openmm as mm; print('OpenMM OK', mm.Platform.getPlatform(0).getName())"
python -c "import openff.toolkit as oft; print('OpenFF OK', oft.__version__)"
```

If you see errors referencing `libstdc++` or `GLIBCXX`, go back to section 3.

---

### 5. Make Rust `pyo3` use this Python

Tell `pyo3`/Cargo which Python binary to use when building or running the Rust bindings:

```bash
# Activate the environment (important)
conda activate atomcad
# Export the Python path for pyo3 builds
export PYO3_PYTHON=$(which python)
# Build your Rust target
cargo build
```

Alternatively, prefix commands with `conda activate atomcad && ...` to ensure the environment is active for the command.

---



---

Build the rust part while atomcad is activated: 

cargo build

run the application:

LD_LIBRARY_PATH="$CONDA_PREFIX/lib:$LD_LIBRARY_PATH" flutter run

## Windows installation summary

On windows installation is more problematic than on Linux and OsX
because openff-toolkit is not available as a simple conda install and it has a reason: The there is no windows version of the Ambertools dpeendency.

Fortunately Ambertools is an optional dependency and so we can install openff-toolkit without it by installing openff-toolkit-base and installing openmm explicitly.

Here is how to install mamba:
https://github.com/conda-forge/miniforge

1. Install miniforge (includes mamba):
   https://github.com/conda-forge/miniforge

2. Create a dedicated conda environment with all dependencies:
   ```
   mamba create -n openff-py311 -c conda-forge python=3.11 openff-toolkit-base rdkit openmm openff-interchange-base packaging -y
   ```

3. For cargo build, set the Python executable:
   ```
   $env:PYTHON_SYS_EXECUTABLE = "C:\ProgramData\miniforge3\envs\openff-py311\python.exe"
   cargo build
   ```

4. For runtime:
.\run_atomcad_win.ps1
or:
.\run_atomcad_win.ps1 -Build

For activating the openff-py311 environment for example for running python unit tests, do this in a miniforge prompt as System Administrator:

mamba activate openff-py311

// run python unit tests whereever your project is:
cd c:\machine_phase_systems\flutter_cad\python
python test_simulation.py
