## Design Proposal
After mulling this over for a while, I think that the design requirements restrict us down to only one real solution. Ignore the previous hints at an implementation unless I mention them explicitly, they were mostly brainstorming.

1. Stable identifiers must exist for every feature and every atom. The FeatureList is effectively unchanged.

2. An `AtomSpecifier` type is introduced with the following fields:
  - `author_id`: The feature ID that actually created an atom (i.e. the feature who's `apply` method created the node)
  - `owner_id`: The feature ID that owns that atom's data (may be equal to `author_id` if this atom was created by a feature that owns atom data)
  - `owner_atom_index`: The atom index in the owner feature where this atom's source data can be found. Note that this does not imply any sort of deduplication or compression of actual graph node data: both this graph node and the owner atom will store an element, position, etc - this field is just for feature tracking.

3. The molecule graph needs to be updated so that each node stores its atom specifier (in addition to the regular atom data).

4. A feature has an `apply` method that accepts a reference to `self`, a reference to the `FeatureList`,  and a mutable reference to a command queue of type `impl MoleculeCommands`. This is quite different than the current design:
  - `apply` does not mutate the feature. This means that it can always be called again on the same inputs and deliver an identical result. (barring bizarre features that involve external factors like timing delays or RNG, which are not considered)
  - The command queue is the only way for a feature to request changes to the molecule or resolve AtomSpecifiers into AtomIndexes. This means that features can be applied virtually, so earlier user actions can be replayed as a part of derived features (the mirror feature can re-run features by providing its own command queue and rerunning the features, rather than just literally mirroring atoms by their coordinates)

5. A feature has a `get_references(&self)` method that returns a list of the `AtomSpecifiers` that it stores.

## Requirements Satisfied

|Requirement | Implementation Details|
|---|---|
|A feature must either describe or enact a list of trivial changes to a molecule (i.e. you must be able to use a feature to realize updates on a molecule)|✅ Command queues are used to mediate molecule changes|
| It must be possible to figure out which features a feature depends on | ✅ this can be done by interrogating `get_references` recursively until root features are found.|
| It must be possible to figure out which feature (features) that an atom depends on | ✅ an atom stores an AtomSpecifier that describes what created it and what feature owns its data. The feature that created an atom can have its lineage traced as just described, so an atom's ancestor features can be found in linear time and with minimal space needs.|
| It must be possible to figure out which features depend on a given feature | ⚠️ This can be done by iterating over every feature and looking at their dependencies, but it is a slow process. Optimizations are possible by storing two-way dependency links between features, but this is out of scope for now.|
|Features must have stable identifiers (i.e. adding / removing features should not change the identifiers of any other feature. Editing a feature should not change its identifier.)|✅ Implemented via the `FeatureList`|
|If a feature is removed, its identifier *may* be reused - removing a feature should explicitly invalidate all of its dependents|4|
|Feature operations should fail gracefully, describe the error, and never lead to a crash|✅ This is just a matter of good code quality and error reporting.|
|It should be possible to reparent a feature and successfully reapply it (e.g. moving a moiety)|✅ |
|It should be possible to create features that reference atoms (e.g. if you want to bond a new atom to an existing one), other features (e.g. if you want to pattern the creation of a feature), or other molecules (e.g. if you want to create a tether between two molecules)|✅ Features can contain arbitrary data (e.g. to reference molecules or files) as well as AtomSpecifiers which robustly track an atom based on its features of origin.|
|It must be possible to specify an atom exactly only by referencing features (without even knowing an AtomIndex). Atoms can be specified by an index ONLY if:<br>- you are indexing a *feature*: e.g. if you create a linear pattern and want to refer to the third copy of a feature, you can index with 3<br>- you are instantiating a set of atoms of non-atomCAD origin (like a PDB file or a base sketch)<br>|7|


TODO: Finish this post.
As I was writing this, I realized that one situation that isn't accounted for is repeatedly copying the same atom. AtomSpecifiers are denoted as author_id.owner_id.owner_atom_index after the atom name
feature 0. make carbon: C[0.0.0]
feature 1. mirror everything: C[0.0.0]-C[1.0.0]
feature 2.mirror everything: C[0.0.0]-C[1.0.0]-C[2.0.0]-C[2.0.0]
After a while you start getting duplicate AtomSpecifiers, which breaks feature tracking entirely
possibly by adding this to AtomSpecifier (but then it gets more confusing with multiple levels of copying)
  - `author_copy_index`: Starts at zero. If the feature that authored this atom created multiple copies of it, this is incremented every time.

Using an author copy index: (author_id.author_copy_index, owner_id.owner_atom_index)
feature 0. make carbon: C[0.0, 0.0]
feature 1. mirror everything: C[0.0, 0.0]-C[1.0, 0.0]
feature 2.mirror everything: C[0.0, 0.0]-C[1.0, 0.0]-C[2.0, 0.0]-C[2.1, 0.0]

One concern I have with this is that it exposes a copy index (which does not seem particularly feature-respecting). I believe it may be possible to create conflicting atom specifiers using this copy index:
feature 0. make carbon: C[0.0, 0.0]
feature 1. Attach silicon: C[0.0, 0.0]-Si[1.0, 1.0]
feature 2. make 3 copies: C[0.0, 0.0]-Si[1.0, 1.0]-C[2.0, 0.0]-Si[2.0, 1.0]-C[2.1, 0.0]-Si[2.1, 1.0]
Now roll back:
feature 3. Attach Germanium: C[0.0, 0.0]-Si[1.0, 1.0]-Ge[3.0, 3.0]
Roll the timeline to the end (the feature order is 0, 1, 3, 2):
C[0.0, 0.0]-Si[1.0, 1.0]-Ge[3.0, 3.0]-C[2.0, 0.0]-Si[2.0, 1.0]-Ge[2.0, 3.0]-C[2.1, 0.0]-Si[2.1, 1.0]-Ge[2.1, 3.0]
Now adjust the pattern feature to only produce 2 copies!
C[0.0, 0.0]-Si[1.0, 1.0]-Ge[3.0, 3.0]-C[2.0, 0.0]-Si[2.0, 1.0]-Ge[2.0, 3.0]
I had expected this might cause a problem: before we started adjusting the timeline there were six atoms; now there are six different atoms. But the owner feature index mitigated this problem (and would do so even if there were multiple atoms in one feature, thanks to the owner atom index.

This scheme even works when multiple copies are made (erasing the copy index of an intermediate feature doesn't cause problems like I suspected it might)
feature 0. make carbon: C[0.0, 0.0]
feature 1. copy twice: C[0.0, 0.0]-C[1.0, 0.0]-C[1.1, 0.0]
feature 2. Copy twice: C[0.0, 0.0]-C[1.0, 0.0]-C[1.1, 0.0]-C[2.0, 0.0]-C[2.1, 0.0]-C[2.2, 0.0]
all of these atoms are still uniquely specified

feature 0. make carbon: C[0.0, 0.0]
feature 1. copy once: C[0.0, 0.0]-C[1.0, 0.0]
feature 2. Copy once: C[0.0, 0.0]-C[1.0, 0.0]-C[2.0, 0.0]-C[2.1, 0.0]
roll back to feature 1, adjust it so that it copies twice: C[0.0, 0.0]-C[1.0, 0.0]-C[1.1, 0.0]
restore to the end of feature 2: C[0.0, 0.0]-C[1.0, 0.0]-C[1.1, 0.0]-C[2.0, 0.0]-C[2.1, 0.0]-C[2.2, 0.0]

feature 0. make carbon: C[0.0, 0.0]
feature 1. copy once: C[0.0, 0.0]-C[1.0, 0.0]
feature 2. Copy once: C[0.0, 0.0]-C[1.0, 0.0]-C[2.0, 0.0]-C[2.1, 0.0]
Note that C[2.1, 0.0] is currently the image of C[1.0, 0.0].
roll back before 1: feature 3. copy once: C[0.0, 0.0]-C[3.0, 0.0]
Now, reapply feature 1: C[0.0, 0.0]-C[3.0, 0.0]-C[1.0, 0.0]-C[1.1, 0.0]
Now, reapply feature 2: C[0.0, 0.0]-C[3.0, 0.0]-C[1.0, 0.0]-C[1.1, 0.0]-C[2.0, 0.0]-C[2.1, 0.0]-C[2.2, 0.0]-C[2.3, 0.0]
After this timeline change, C[2.1, 0.0] is actually the image of C[3.0, 0.0]! This means that the 4-field atom specifier diverges from a full atom lineage specification.

By contrast to the above, if we specify atoms with the tedious full lineage (C[0.3 1.4 2.5] means the sixth copy (index 5) of the third feature's (index 3) copy of (the fifth copy of the second feature's copy of (the fourth atom in primitive feature 0, the first feature))":

Feature 0: C[0.0]
Feature 1: C[0.0]-C[0.0 1.0]
Feature 2: C[0.0]-C[0.0 1.0]-C[0.0 2.0]-C[0.0 1.0 2.0]
Here, C[0.0 1.0 2.0] is the image of C[0.0 1.0]
Roll back and add feature 3: C[0.0]-C[0.0 3.0]
Move back to end: C[0.0]-C[0.0 3.0]-C[0.0 1.0]-C[0.0 3.0 1.0]-C[0.0 2.0]-C[0.0 3.0 2.0]-C[0.0 1.0 2.0]-C[0.0 3.0 1.0 2.0]
C[0.0 1.0 2.0] is still the image of C[0.0 1.0]: all is good :)

* NOTE: feature lineage -> atom lineage. This isn't about feature dependency
I am still unconfident this model is correct, because I arrived at it by trial and error rather than finding a way to show its equivalence to the full feature-lineage list. I think it may be possible to prove divergence:
- For an atom to be robustly specified, it must be uniquely associated with an atom lineage.
- An atom lineage is an ordered subset of the feature list
- If a change to the atom lineage or feature list causes an atom specifier to become invalid, it is because the atom of interest that it referred to is now ambiguous (user action targets are modelled via the atom specifier, with large topology changes we cannot safely reproject the user's intentions)
- Features in an atom lineage will always be related via a dependency chain, with each newer feature depending on the feature before it. For example, in lineage x, y, z, if feature z created the copy of some atom we care about, then it either copied it from feature x or feature y. If it was copied from feature x, then feature y would not be relevant in this atom specifier. So the atom must be copied from feature y, and thus feature y is a dependency of feature z. The same logic allows us to infer that feature y got its copy of the atom from feature x, and thus z depends on y depends on x (and this can be inductively applied to every atom lineage.
- To be more precise, if an atom was created by feature z, the atom lineage will be exactly equal to the ordered chain of recursive feature dependencies of feature z. i.e. if feature z depends on features x and y, and feature x depends on feature w, then the feature lineage of z will be exactly: w, x, y, z
- This means that to specify feature lineage, you only have to specify 
- Because features in a feature lineage are in this dependency relationship, they can *never be reordered*. AtomCAD will always prevent you from moving z before y, because z depends on y.
- This means that the feature lineage can be restricted: it is not a only an ordered subset of the global feature list, it is an ordered subset of the global feature list truncated at feature z (the last feature in the lineage). So if the global feature list is a, b, c, d, e and an atom was created by feature c, its feature lineage is an ordered subset of a, b, c.
- so we can specify an atom lineage by specifying the atom and feature where its data comes from (the start of the ordered subset) and the end atom and feature we derived (the end of the ordered subset)

I think we can prove information theoretically that there is no way to robustly shorten an atom lineage. Instead, we must make use of heavy caching and use small integer sizes to prevent excessive memory use (this introduces limitations like "a feature cannot make more than 255 - i.e a u8 - copies of something). For now I will not do this.