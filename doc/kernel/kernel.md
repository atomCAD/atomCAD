# Kernel

- Written in Rust.
- Simple model representation (atoms, bonds and group tree)
- Stores edit history tree (saved with model to file)
- Unit testable
- Called Kernel for simplicity, but it is more and editor backend, as it also deals with tool state and selection state

## History implementation

There are several known strategies to implement undo-redo history in an editor application. (*Command pattern*, *State snapshots*, *Change logs*). I chose the **Command Pattern**, because it is most performant among the three mentioned approaches. The drawback of the command pattern is that implementation of the commands can be complex and needs careful consideration, but I think the command pattern is a good fit because our model is relatively simple. Also, although writing commands need some consideration, very good tests can be written for them. I will write tests for each command from day one.

The application also need to refresh the rendered representation according to the changes. Efficient rendering approaches need to know what has changed. The renderer could find out these changes by having a snapshot from the previous frame and do a comparison each frame, but it would be inefficient. So it is also the responsibility of our history implementation to report the frame-by-frame deltas to the renderer. This delta reporting can be *conservative*: it is not a big problem if we report a change where change did not happen, but it would be a problem the other way around. the kernel will simply report a conservative set of dirty atom ids where change might have happened. Reporting such a dirty atom id set is very simple in the Kernel. If an atom was deleted, created or changed its id becomes dirty. If a bond was deleted, created or modified its atoms become dirty.

## Commands

- Select
- Delete selected
- Copy selected
- Add atom
- Add bond
- Move
- Rotate

## Model Representation, Ids

Groups, atoms, and bonds have ids.

The bond list of an atom consist of a vector of ids. A bond knows the id of its two atoms.

In the kernel an atom or a bond can be quickly accessed by id, because we store atoms in an id -> atom map and bonds in an id -> bond map. Bonds can also be accessed quickly by two atom ids, because it is quick to access one of the atoms, and the list of bonds for an atom is small.

Ids also play a key role in our undo-redo history implementation. Commands rely on atom and bond ids both in terms of what to do during execution and also during undo (undo information). The contract for commands is that commands need to be implemented in a way that when going to a time point in history using undo and redo operations, at that point the atoms and bonds should have the exact same ids deterministically as they had when originally reached the given step. Example: when atoms with certain ids are deleted in a command, the undo of the command stores those atoms along with their ids and so they will be restored to the same atom ids. Similarly, the redo of an 'add atom command' will use the same id as before, so that later command referring to this id will refer the correct atom.

