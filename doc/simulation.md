# Simulation considerations in atomCAD

We need energy minimization and dynamic simulation capabilities in atomCAD. atomCAD supports generic unit cells with generic motifs. Accurate, fast minimization and dynamic simulation for generic large atomic structures is impossible today. Therefore we need compromises. This document tries to evaluate the future simulation use-cases in atomCAD and the possible techniques to use in case of these use-cases. 

## Simulation models

There are 4 broad categories of atomic simulation models which need to be considered. In the order of increasing accuracy and decreasing speed:

- Fix-topology force fields 

- Reactive force fields

- Machine-learned interatomic potentials

- Quantum mechanics simulations

Of these, quantum mechanics simulations are too slow to use for atomCAD use cases, so we do not deal with them in this document anymore. Let's study the other 3 categories:

### Fix-topology force fields

These are force fields in which no new bonds are created. Typical forces simulated:

- bond stretching force
- bond angle force
- van der Waals force (e.g. Lennard-Jones)

#### UFF

A simple fix-topology force field to implement from scratch in atomCAD is UFF. It is inaccurate but at least supports almost the whole periodic table.

Can be implemented based on the original paper or based on the implementation in OpenBabel.

#### OpenFF

The OpenFF organization defined the SMIRNOFF standard to define fix-topology force fields in .xml, and they maintain an official OpenFF force field .xml file in this same format. It is more accurate than UFF but less general. Msep created some extensions to this also in the SMIRNOFF format, mainly to better support silicon, but even with the extension it is still less general than UFF. We do not necessarily need to have full support for the whole SMIRNOFF standard (the standard of the openFF .xml file format) but we might convert most of the forces in the official OpenFF .xml file and also the ones in Msep extension to our own representation and use it in our own engine.

### Reactive force fields

In reactive force fields new bonds can be created dynamically.

The big question is to what extent can fix-topology force fields give us the accuracy we need in case of mechanical simulations of atomic crystal parts, and to what extent are reactive force fields needed?

As far as I know:

- Already fix-topology force fields can give elastic response, vibrations and sliding where nothing chemically changes. They can give us large-scale mechanical contact that’s purely mechanical.
- But fix-topology force fields **cannot** model bond breaking/forming, tribochemical wear, rehybridization, or material transfer -  all of which are central to realistic wear and many friction phenomena. Here reactive force fields are needed.

Here are some of the models in increasing complexity:

#### Stillinger-Weber

Originally developed for silicon but with appropriate parameters can be applied to carbon diamond too. It works towards maintaining tetrahedral coordination of the silicon/carbon atoms. There is no explicit handling of bonds here: bonds are present implicitly when atoms get close to each other. (Tetrahedral coordination is maintained purely geometrically). This means topology can change if large forces cause atoms to rearrange and bond to different neighbors.

Simple algorithm, we can roll our own implementation.

#### AIREBO

A reactive force field for **carbon and hydrogen only**. Rolling our own would be nontrivial amount of work. It can be reused from the LAMMPS library from Rust through a C/C++ interface.

**Important limitation:** AIREBO does not support silicon - only suitable for pure diamond structures without Si substrates or Si-C mixed systems.

#### Tersoff

A reactive force field with environment-dependent bond order, widely used for silicon and carbon. Available in LAMMPS. Good middle ground between Stillinger-Weber (simpler, faster) and ReaxFF (more complex, slower). Works well for pure Si or pure C systems, but parametrizations are element-specific - not suitable for mixed Si-C interfaces.

#### ReaxFF

A more complex reactive force field than AIREBO, but supporting many more elements. ReaxFF should be used when elements beyond C-H are needed (e.g., silicon-carbon systems). Requires parameter sets for specific element combinations. 

### Machine-learned interatomic potentials

#### UMA

UMA (Universal Models for Atoms) is a state-of-the-art family of machine-learned interatomic potentials developed by Meta FAIR, released in 2025. UMA represents a major advancement in universal atomic modeling.

**Key characteristics:**

- **Training scale:** Trained on ~500 million unique 3D atomic structures (largest training run to date), covering molecules, materials, and catalysts
- **Novel architecture:** Uses "Mixture of Linear Experts" (MoLE) design, which enables high model capacity without sacrificing inference speed
- **Universal scope:** Single model works across multiple chemical domains without fine-tuning
- **Open source:** Code, weights, and data released by Meta FAIR (available on Hugging Face: facebook/UMA, integrated with FAIRChem)

**Model variants:**

UMA comes in multiple sizes (small, medium, large). For atomCAD, we will focus on **UMA small** as the primary target:

- **UMA small:** ~150 million parameters
  - Small memory footprint suitable for desktop applications
  - Performance: ~16 minimization iteration steps per second for 1000 atom structures
  - Good balance of accuracy and speed
  - **This is our initial target implementation**

- **UMA medium:** ~1.4 billion total parameters (but only ~50M active per structure due to MoLE)
  - Higher accuracy, moderate speed cost
  - Can be supported later as optional "high accuracy" mode

- **UMA large:** Highest accuracy, slower
  - Can be considered for future enhancement

**Why UMA for atomCAD:**

- State-of-the-art accuracy approaching DFT for many properties
- Trained on diverse chemical systems (molecules, crystals, surfaces)
- Faster than alternatives with similar accuracy (e.g., MACE-MP-0)
- More recent than other universal potentials (ORB, CHGNet, M3GNet)
- Designed explicitly for production use cases

**Integration approach:**

UMA can be integrated into Rust without a Python dependency:

Typical deployment for PyTorch models in non-Python apps is: **export** to TorchScript (or ONNX), **load** the serialized model from C++/other runtime, and **call forward** from Rust via an FFI-equipped client (tch-rs or ONNX runtime Rust bindings).

Local inference (embedding PyTorch in atomCAD) presents significant deployment challenges:

- **GPU inference:** Requires ~2GB CUDA dependencies (libtorch_cuda + CUDA runtime)
- **CPU inference:** Smaller but still significant (~300-500 MB libtorch CPU libraries)
- **ONNX runtime:** May offer some size optimization, but model compatibility needs investigation

**Client-server architecture decision:**

Due to the deployment size and complexity issues, we have decided to use a **client-server model** for UMA simulations:

- atomCAD client connects to a UMA inference server (local or remote)
- Server handles model loading and GPU-accelerated inference
- Keeps atomCAD installation lightweight
- Enables GPU acceleration without requiring user GPU hardware
- **Open question:** Whether the server implementation will be open source or proprietary to Machine Phase Systems (developer of atomCAD)

### Summary comparison of simulation models

| Model | Type | Element Coverage | Reactive | Relative Speed | Relative Accuracy | Implementation |
|-------|------|------------------|----------|----------------|-------------------|----------------|
| **UFF** | Fix-topology | Nearly all elements | No | Very Fast | Low | From scratch or OpenBabel |
| **OpenFF** | Fix-topology | Limited (organic molecules, some Si extensions) | No | Very Fast | Medium (better than UFF) | Custom engine from .xml |
| **Stillinger-Weber** | Reactive | Si, C (separate params) | Yes (implicit) | Fast | Medium | From scratch |
| **AIREBO** | Reactive | **C, H only** | Yes | Fast | Medium-High | LAMMPS library |
| **Tersoff** | Reactive | Si or C (element-specific) | Yes | Fast | Medium-High | LAMMPS library |
| **ReaxFF** | Reactive | Many elements (needs param sets) | Yes | Medium | High | LAMMPS library |
| **UMA small** | ML potential | Universal (all trained elements) | Yes | Slow (CPU) / Medium-Slow (GPU) | Very High (near-DFT) | Client-server (GPU) |

**Key notes:**
- **Element coverage** is critical for atomCAD - AIREBO (C-H only) and Tersoff (element-specific) have significant limitations
- **Reactive** means the model can handle bond breaking/formation during dynamics
- **Speed** is relative to model complexity; actual performance depends on system size and hardware
- **UMA** offers the best accuracy-generality tradeoff but requires server infrastructure
- **UFF** is the simplest option for broad element coverage with fast performance
- **Stillinger-Weber** topology changes are geometry-based, not chemically reactive in the traditional sense

## Future option: UMA-matched hybrid force fields

A promising future direction is to use UMA as a high-quality reference to create **fast, accurate, domain-specific force fields** that combine the speed of classical methods with UMA-level accuracy for atomCAD's specific use cases.

### Concept

Instead of running UMA directly (which is slow) or using generic classical force fields (which are inaccurate), we can:
1. Generate large datasets of energies and forces from UMA for atomCAD-relevant structures (diamond, silicon, defects, surfaces)
2. Fit specialized classical force fields to reproduce UMA's behavior
3. Create a hybrid system that automatically blends different force fields based on the local atomic environment

### Three-tier approach

**1. Specialized reactive potentials (fast, production-ready)**
- **Tersoff**: Fit to UMA for perfect diamond/silicon crystals and strained lattices (default fast option)
- **AIREBO**: Fit to UMA for C-H surface chemistry and passivation
- **ReaxFF**: Opt-in for heavy reactivity when true chemical reactions are needed
- Parameters optimized via force matching against UMA reference data

**2. Generic tabulated many-body fallback**
- Flexible spline-based potentials: 2-body radial + separable 3-body angular terms
- Covers arbitrary covalent motifs not handled by specialized potentials
- More accurate than UFF for diverse defects, much faster than UMA
- Can be exported to LAMMPS `table` format

**3. Intelligent blending strategy**
- **Energy-based smooth blending** (preferred): Per-atom weights based on "crystal-likeness" (coordination, local order parameters)
- **Delta learning** (simpler): Baseline classical force field + small correction table for residuals
- Ensures smooth transitions between different potential regions

### Implementation path

**Data generation:**
- Generate diverse UMA reference data via FAIRChem/ASE
- Cover bulk crystals (strains, temperatures), surfaces, vacancies, interstitials, defect clusters
- Include transition/interface snapshots for blending regions

**Fitting and validation:**
- Use force matching tools (potfit or similar) to optimize parameters
- Validate: lattice constants, elastic moduli, surface energies, defect formation energies, phonons
- Test energy conservation (NVE stability)

**Practical rollout:**
- Provide presets: "Fast" (Tersoff-UMA), "C/H surfaces" (AIREBO-UMA), "Reactive" (ReaxFF-UMA), "Generic" (tabulated), "Hybrid" (blended)
- Implement blending in custom LAMMPS `pair_style` or use `hybrid/overlay`

### Benefits for atomCAD

- **Speed**: Classical force field performance (much faster than UMA)
- **Accuracy**: Trained on UMA, so inherits UMA-level accuracy for covered configurations
- **Generic**: Tabulated fallback handles unexpected atomic environments
- **No server required**: Runs locally, unlike UMA client-server
- **Domain-optimized**: Specifically tuned for mechanical nanomachines (diamond, silicon)

### When to develop this

This is an advanced method requiring significant research and validation. Recommended timeline:
1. First implement UMA client-server (reference accuracy)
2. Gain experience with what configurations are common in user workflows
3. Generate UMA training data for those specific patterns
4. Develop and validate UMA-matched force fields for production use

## Simulation use cases in atomCAD and suggested solutions

I can see 4 different simulation use cases in atomCAD:

- It would be nice to have a special fast method for better hydrogen passivation to avoid steric clashes.
- A generic energy minimization node. Need not be that fast.
- Semi-realtime energy minimization during direct atom editing (as in Samson)
- Dynamic mechanical simulation. Need not be that fast.

We will list all these use cases and suggest a solution for them.

#### atomCAD use case 1: Steric clash avoidance during hydrogen passivation

A very simple force field is enough here which only moves the hydrogens and supports 3 forces:

- bond stretching
- bond angle bending
- repulsive part of Lennard-Jones.

Even using such a simple force field with an integrator is not needed here. We can achieve a similar effect with a simple geometric optimization algorithm:

For all hydrogens i, j:

- For each overlapping pair (i,j) with `r_ij < d_min` produce a displacement:

  - compute `delta = (d_min - r_ij) * 0.5` along unit vector `u = (r_i - r_j)/r_ij`.

  - add `+delta * u` to `r_i_accum` and `-delta * u` to `r_j_accum`.

- Apply accumulated displacements to H positions, but **cap step length** per iteration (e.g. max 0.1 Å).

- **Project each H back** to bond distance `r0` from its parent atom (if we want fixed length) or clamp to `[r0 - dr, r0 + dr]`.

- Apply a small pull-back toward original bond direction: `r_new = normalize(r_pos - C) * r0`; then `r_pos = lerp(r_pos, C + r_new, alpha_ang)` with small `alpha_ang` (0.1) so hydrogens slowly prefer original direction.

#### atomCAD use case 2: A generic energy minimization node

This node will support multiple simulation methods:

**Primary option: UMA small** via the client-server architecture.
- The atomCAD client will send the atomic structure to the UMA server, which performs GPU-accelerated energy minimization and returns the optimized structure.
- Highest accuracy, but requires server connection.

**Alternative option: UFF**
- Since we will implement UFF for the semi-realtime editing feature anyway, offering it as an energy minimization option is free.
- Much faster than UMA, but lower accuracy.
- Good for quick initial optimizations or when server is unavailable.

**Future option: Tersoff** (via LAMMPS integration)
- Middle ground between UFF and UMA for carbon and silicon structures.
- Faster than UMA, more accurate than UFF.
- Could be integrated through LAMMPS library.

**Advanced future option: UMA-matched hybrid force fields**
- Combines speed of classical methods with UMA-level accuracy.
- Uses UMA to train domain-specific force fields for atomCAD's typical structures.
- See dedicated section above for details.

This can be several seconds for bigger atomic structures, so before implementing this we should support long running nodes by doing the node network evaluation on a separate thread.

The UX difference will be the following:

- Currently running nodes will be denoted with a hourglass or other marking.
- Execution of nodes can be cancelled.

#### atomCAD use case 3: Semi-realtime energy minimization during direct atom editing

We most probably need a separate direct atom editor in atomCAD for multiple reasons:

- There are direct atom editing use cases where the non-descriptive workflow does not seem to help: it only makes the UI more complicated.
- We need a motif editor anyway, so why not create a direct atom editor which has special capabilities tailored to motifs (like bonding between neighboring cells, showing atoms from neighboring cells)
- We need a defect editor anyway, so why not create the direct atom editor with defect editing convenience functionality: a feature where during editing the defect you can have a crystal structure as a background layer in the editor.

In this separate direct atom editor we should have semi-realtime energy minimization to make user-edited structures near-physical all the time (as done in Samson). We have high genericity and high speed requirements here so we cannot go with the accurate models:

I think the best option is our custom implementation of UFF with some additional forces geared towards accurate forces for diamond.

We should develop our own UFF variant because it is relatively simple and we need to customize this.

To make the development very fast we should develop a CPU variant first, and once satisfied with its parameters we should port it to work as a compute shader.

#### atomCAD use case 4: Dynamic mechanical simulation

This need not be fast, this can be a long-running node.

We will use UMA small via the client-server architecture.

Complex reactive force fields could be an option but they need a very high expertise to parameterize correctly and are still less generic than UMA small. We might consider the reactive force fields later.

**Advanced future option: UMA-matched hybrid force fields**

As detailed in the dedicated section above, we could develop UMA-trained classical force fields that combine:
- Fast performance (classical force field speed)
- High accuracy (trained on UMA reference data)
- Domain optimization (specifically for mechanical nanomachines)

This approach would use force matching to fit Tersoff/AIREBO/ReaxFF parameters to UMA's behavior on atomCAD-relevant structures, plus tabulated many-body potentials for generic fallback. The system would intelligently blend different force fields based on local atomic environment.

This would eliminate the need for server infrastructure while maintaining near-UMA accuracy for common atomCAD workflows.

Important aspect: The client-server protocol must be designed to return simulation trajectories as efficiently compressed frame deltas rather than full frames, minimizing bandwidth and enabling efficient playback on the client side. 

### Parameter tuning and model testing considerations

In all cases when we roll our own solution versus reusing a library we first implement a CPU version quickly to be able to test it and fine-tune parameters.    

It is a good idea to integrate UMA first as it can give us a good target for parameter tuning the other models.

It is also a good idea to support an atomic structure comparison node to help compare results. Also assessing coordinates of atoms and bond lengths should be simple on the UI.

## Roadmap

- Improve this document by better understanding the capabilities of the existing models vs. atomCAD needs. (For example during mechanical simulation of mechanical machines created in atomCAD: which model can simulate the physical aspects that are relevant in the specific use-case)?
- Implement node network evaluation on a different thread and make the evaluation asynchronous from the UI perspective.
- Create the energy minimization node by integrating UMA small (using client-server architecture).
- Add comparison and measurement features into atomCAD.
- Add the fast steric avoidance algorithm for Hydrogen passivation.
- Implement the direct atom editor in atomCAD (supporting motif editing and defect editing)
- Integrate semi-realtime UFF-based energy minimization into the direct atom editor.