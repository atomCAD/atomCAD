// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

/// The identifier for an edit operation.
pub type EditId = usize;

/// An identifier that represents a specific instance of some patterned edit. For example,
/// assume edit 0 places one atom. Edit 1 creates two copies of edit 0 - there are now
/// three total atoms. Edit 0 is not really a pattern, but in a way you can imagine that it
/// owns one copy of itself (this is sort of an "identity pattern*"). The second atom, however,
/// is the first copy (the first *instance*) that edit 1 makes of its target (which is edit 0).
/// So, the second atom belongs to the pattern instance identified by
/// `PatternInstanceId { owner_id: 1, instance: 0 }`. The third atom is created by the second
/// copy of the target geometry in edit 1, so it belong to the pattern instance identified by
/// `PatternInstanceId { owner_id: 1, instance: 1 }`.
///
/// This is an essential part of uniquely labelling each atom produced by an edit list -
/// see `AtomSpecifier` to understand its role in that context.
///
/// * The "identity pattern' mentioned would be identified as `PatternInstanceId { owner_id: 0, instance: 0}`
///   in this example.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PatternInstanceId {
    pub owner_id: EditId,
    pub instance: usize,
}

/// An `AtomSpecifier` is used to uniquely label each atom in a molecule produced by applying a list
/// of edit operations. In addition to being unique, every atom's specifier must firmly
/// associate it with the edits that make up its lineage. This makes each atom into a stable
/// anchor point that other edit operations can reference, even if the edit list's "past" is
/// altered or recomputed.
///
/// Achieving these goals is harder than it sounds. Most optimizations will subtlely break one
/// of the desired properties, leading either to multiple atoms having the same identifier, or,
/// when the "past" is altered, causing an edit to "teleport" to some seemingly random location.
///
/// Uniqueness is a relatively easy property to ensure - each new atom can just increment a counter.
/// However, this is obviously inefficient when we alter the past:
///
/// Fig 1. Edit dependency failure
/// edit 0: create atom 0 (C)
/// edit 1: create atom 1 (target atom id = 0) (C-C)
/// edit 2: passivate the terminal carbon (target atom id = 1) (C-CH3)
/// Now, the user steps back in the timeline, creating another branch. The timeline becomes:
/// edit 0: create atom 0 (C)
/// edit 3: add the new branch, say a nitrogen. N is atom 1 now, as we're rebuilding from scratch (N-C)
/// edit 1: create atom 1 (target atom id = 0) (N-C-C) (this still is fine, atom 0 is a central carbon)
/// edit 2: passivate the terminal carbon (target atom id = 1) (NH2-C-C)
/// Notice how, because the order of atom creation was changed, index 1 stopped referring to the terminal
/// carbon! We passivated the nitrogen instead - the passivation edit has "teleported" and the user is
/// confused.
///
/// The problem in Fig 1. can be solved by adding a new field to the atom specifier. Instead of storing
/// a global atom index (which is unique, but "teleports" when the past changes), we store the
/// edit ID (i.e. "edit n" has edit ID = n) and the child index. The child index is incremented for
/// every new atom that an edit operation creates - for example, the passivation edit created three
/// H atoms - they have specifiers like `edit id = 2, child index = 0, 1, 2`. This solves the problem:
///
/// Fig 2. Edit ID + child index
/// Here, atom specifiers are written as the tuple (edit id, child index)
/// edit 0: create atom (0, 0) (C)
/// edit 1: create atom (1, 0) (target atom = (0, 0)) (C-C)
/// edit 2: passivate the terminal carbon (target atom = (1, 0)) (C-CH3)
/// Now, the user steps back in the timeline, creating another branch. The timeline becomes:
/// edit 0: create atom (0, 0) (C)
/// edit 3: add the new branch. N is atom (3, 0) (N-C)
/// edit 1: create atom (1, 0) (target atom = (0, 0)) (N-C-C)
/// edit 2: passivate the terminal carbon (target atom = (1, 0)) (N-C-CH3)
/// Notice how the passivation `Edit` did not teleport this time - it stayed on the terminal
/// carbon, where we wanted it.
///
/// This system seems sufficient - and, in fact, it is, if we banned all copying-related `Edit`s (
/// like patterning, mirroring, or duplication). Let's see why a simple copy operation breaks things.
/// To keep the example short, I'm adding a naive pattern `Edit` that creates disconnected copies, but
/// this example still holds when each copy is bonded to the original - it is just harder to follow.
///
/// Fig 3. Edit ID + child index v.s. Copying
/// edit 0: Create a 1-long chain (creates atom (0, 0)) (C)
/// edit 1: Make a linear pattern from edit 0 (Creates atoms (1, 0..3)) (C  C  C)
/// edit 2: Passivate the carbon in the third copy (target atom = (1, 1)) (C  C  CH3)
/// Now, the user steps back to just after edit 0. They update the chain length to 2! The timeline is now:
/// edit 0: Create an n-carbon chain, n=2. (Creates atoms (0, 0), (0, 1)) (C-C)
/// edit 1: Make a linear pattern of edit 0 (Creates atoms (1, 0..4)) (C-C  C-C  C-C)
/// edit 2: Passivate the first carbon in the second copy (target atom = (1, 1)) (C-C  C-CH3  C-C)
/// Notice that the passivation has jumped from being on the second copy to being on the first copy!
///
/// The problem in Fig. 3 occurs because `Edit`s that copy external geometry act as an escape hatch
/// from our indexing system. Earlier, we associated an edit ID with each atom, so that altering the
/// "past" (and thus changing a global atom ID counter) was no longer a problem - we namespaced the
/// atom id counter to each `Edit`. When edit 1 copied the data from edit 0, it let edit 0 increase
/// its local atom ID counter! Just like that, we accidentally created a global atom counter, and
/// now our AtomSpecifier is not stable.
///
/// Now, we're forced to arrive at an `AtomSpecifier` that looks very much like the real
/// implementation below. We can't just store an edit ID and a counter, because a copied
/// `Edit` will increase that counter an unpredictable amount. A somewhat reaonsable workaround
/// is to reset the child_index to 0 not just for every `Edit`, but for every *pattern instance*.
/// For example, if the passivation step in Fig. 3 specified its target atom by "the first atom
/// in the second copy created by edit 1", the showcased problem would not occur - the final
/// result would be (C-C C-C CH3-C) as intended. In effect, this is what `PatternInstanceId`
/// is for. It stores not just the `Edit Id` an atom is associated with, it also knows which
/// pattern instance we are referring to.
///
/// Unfortunately, even that is not enough! In fact, no matter how hard we try, the idea of
/// an atom id counter is always susceptible to stability issues.
///
/// Fig 4. PatternInstanceId + child index v.s. Repeated Patterning
/// This is a complex example. To make things easier, AtomSpecifiers are written next to the
/// atom in the drawing. C[a.b c] has edit ID a, pattern instance index b, and child index c.
/// edit 0: Create one atom (C[0.0 0])
/// edit 1: Copy everything (C[0.0 0]  C[1.0 0])
/// edit 2: Bond a hydrogen to C[1.0 0]:  (C[0.0 0]  C[1.0 0]-H[2.0 1])
/// edit 3: Copy everything (C[0.0 0]  C[1.0 0]-H[1.0 1]  C[3.0 0]  C[3.0 1]-H[3.0 2])
/// Notice that C[3.0 1] is attached to a hydrogen atom.
/// Now, the user steps back before edit 1, and introduces a new copy operation:
/// edit 0: Create one atom (C[0.0 0])
/// edit 4: Bond a carbon to C[0.0 0] (C[0.0 0]-C[4.0 0])
/// edit 1: Copy everything (C[0.0 0]-C[4.0 0]  C[1.0 0]-C[1.0 1])
/// edit 2: Bond a hydrogen to C[1.0 0]:  (C[0.0 0]-C[4.0 0]  H[2.0 1]-C[1.0 0]-C[1.0 1])
/// edit 3: Copy everything (C[0.0 0]-C[4.0 0]  H[2.0 1]-C[1.0 0]-C[1.0 1]  C[3.0 0]-C[3.0 1]  H[3.0 3]-C[3.0 2]-C[3.0 4])
/// Notice that C[3.0 1] is *not* attached to a hydrogen atom!
///
/// The issue in Fig. 4 is somewhat more abstract than the previous examples, but it
/// shows an issue with this system. Our PatternInstanceId protected against a single
/// layer of patterning, but we were able to pollute even this more localized child
/// index by applying two laters of patterning. This is a general trend, too - if
/// we want to protect against N layers of patterning, we need to namespace our
/// child counter behind N `PatternInstanceId`s.
///
/// This is how we arrive at the solution that atomCAD uses. Every time an atom is
/// copied by a edit, it's `AtomSpecifiers`'s `path` has a new `PatternInstanceId`
/// pushed to the back, describing which edit made that copy and what the current
/// instance index (the copy number inside that edit) is. The child index is never
/// changed (which means targets never invalidate), and originally comes from some
/// primitive. For example, a primitive edit like "place a benzene ring" would create
/// atoms with child indexes from 0 to 5. However, adding a edit that patterns an
/// atom 10 times would never change the child index - it would just change the outermost
/// `instance` counter in the `path`.
///
/// Because the full lineage of every atom is stored, we guarantee that each AtomSpecifier
/// is stable (no matter how we alter the timeline - unless you reorder or delete a dependency).
/// Additionally, because we store instance ID and child indexes, we guarantee
/// uniqueness.
///
/// The downside of this system is that it has a large memory footprint (each atom stores a
/// `Vec`) and caution must be taken when writing `Edit` implementations. Although optimization
/// is possible (using trees to cache the paths, for example), it seems impossible to
/// avoid tagging every atom with its full edit lineage.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct AtomSpecifier {
    pub path: Vec<PatternInstanceId>,
    pub child_index: usize,
}

impl AtomSpecifier {
    // Creates the trivial AtomSpecifier for the first atom created by edit `owner_id`.
    pub fn new(owner_id: EditId) -> Self {
        AtomSpecifier {
            path: vec![PatternInstanceId {
                owner_id,
                instance: 0,
            }],
            child_index: 0,
        }
    }

    // Uses this `AtomSpecifier` like an iterator: mutates self to increment the child index,
    // and returns a clone of this AtomSpecifier that can be used to name an atom.
    pub fn next_spec(&mut self) -> Self {
        let ret = AtomSpecifier {
            path: self.path.clone(),
            child_index: self.child_index,
        };

        self.child_index += 1;
        ret
    }
}
