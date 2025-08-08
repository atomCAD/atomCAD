## How should we do energy minimization?

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

## Summary

Our current planned approach:
- Use OpenMM with the merge of the OpenFF 2.2.1 force field and the MSep One extension 0.0.1 to do energy minimization if it supports our molecule.
- Use UFF as a fallback as described above.

