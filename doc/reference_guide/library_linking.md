# Linked libraries

A design can **link** other `.cnnd` files. The networks and record types of a
linked file — a *library* — appear in your design and can be used like your own
(as custom nodes, as record types, as function values), but they stay in their
own file: you cannot edit them from here, and they are never saved into your
design. When the library file changes, your design picks up the new version.

This is different from *File > Import copy…*, which copies networks into your
design once and forgets where they came from.

## Linking a library

*File > Link library…* picks a `.cnnd` file and asks for an **alias** — the
folder the library's content appears under. The alias defaults to the file name
without a version suffix (`demolib_v3.cnnd` suggests `demolib`), so the
library's network `half_space` becomes `demolib.half_space`. An alias may
contain dots (`libs.demolib`) to group several libraries in one folder; each
part must be a name made of letters, digits and underscores, not starting with
a digit, and it may not clash with anything already in your design.

The design must have been saved once first: the library is remembered by its
path **relative to your design's file**, so the pair can be moved together
(`../libs/demolib.cnnd` is fine). A file on another drive has no relative
path; the dialog then says so and offers **Copy and link** instead: the file is
copied next to your design (together with the libraries and data files it uses,
each landing where the library's own relative paths expect it) and the copy is
linked. Nothing is copied if a different file with the same name is already in
the way.

Linking is one undo step.

## Linked content in the Node Networks panel

A linked library shows as a folder labelled with its alias and, smaller, its
file name: `demolib — demolib_v3.cnnd`. Everything under it is dimmed and
marked with a link icon, and hovering a row shows the file it comes from. If
the library itself links other libraries, they appear as folders inside it
(`demolib.common — common.cnnd`). Those are the library's own business: they
can be browsed, but they are not offered in the *Add Node* popup — link a
library directly if you want to use it.

Right-click menus on linked content offer only what does not change it:

- a linked **network**: *Find Usages*, *Open library file*, *Duplicate into my
  file* (a local, editable copy at the top level of your design; its references
  to other library networks stay links);
- a linked **record type**: *Open library file*;
- the **library folder**: *Refresh*, *Open library file*, and for a library you
  linked directly *Change file…*, *Rename alias…*, *Make local copy* and
  *Unlink*.

Linked rows cannot be renamed, moved, deleted or dragged, and nothing can be
created or dropped inside a library's folder. A local folder that holds a
linked library cannot be renamed or moved either, as that would change the
alias.

### Status badges

- A **red error badge** on the library folder means the file is missing,
  cannot be read, or links back to a file that links it (a cycle). Hover it
  for the reason; click it to copy the reason. Nothing is removed: your nodes
  that use the library keep their wires and settings and show an error until
  the file is back.
- A small **"older than disk" marker** (a clock with an arrow) means the
  version in use is not the one on disk — after you undid a refresh, or while a
  refresh is held because there is something to redo (see below). Click it to
  refresh.

## Browsing a linked network

Clicking a linked network opens it like any other: canvas, properties panel
and 3D view. A strip above the canvas says where it is linked from, with an
*Open library file* button. You can select nodes, read hover values, copy
nodes, toggle what is displayed in the 3D view and move around; you cannot add,
delete, move or wire nodes, and the property editors are greyed out. What you
change while browsing (camera, canvas position, displayed nodes) is not saved
and does not mark your design as changed.

To edit a library, open it: *Open library file* replaces your design with the
library file (you are asked to save changes first). Edit it and save it, then
use *File > Back to …* to return to your design, which picks up the new
version as it opens.

## When a library changes

atomCAD notices when a linked library — or a data file one of your nodes reads,
such as an `.xyz` — changes on disk: when the window regains focus, and every
couple of seconds while it has focus (so a script or a `git pull` running in
the background is noticed too). It does not check in the middle of a drag, while
you are typing, or while a dialog is open.

A change is brought in automatically, as **one undo step**:

- If nothing of yours was affected, a short message says so: *Refreshed
  demolib (libs/demolib_v3.cnnd)*.
- If the new version disconnected wires (a parameter was removed), changed the
  type of a wired parameter, or dropped a network or record type you use, a
  message stays until you close it: *Refreshed demolib — 3 wires
  disconnected*, with **Details** (a list of every affected wire and node;
  click a row to go there) and **Undo**.

Wires follow the library's parameters by identity, not by position: a library
that reorders, renames or inserts parameters keeps your wires on the right
pins. Only a parameter that no longer exists loses its wire, and that is always
reported. Output pins are matched by position, so a library that reorders its
output pins can move the wires leaving them — the report lists every such wire
for you to check.

A node that uses a network or record type the library no longer defines is
kept exactly as it was — wires and settings — and shows *Unknown node type* or
*Unknown record type*. It comes back to life, wires intact, when the name
returns (the file is restored, or you point the alias at a file that has it).

**While there is something to redo**, a change is *held* rather than applied, so
it cannot wipe out your redo history: a message says *demolib changed on disk —
refresh held while redo is available*, with a **Refresh** button, and the
library folder shows the "older than disk" marker. The next edit you make
applies it.

**Undo** of a refresh puts the previous version back; the folder then shows the
"older than disk" marker and no automatic refresh follows. *Refresh* (on the
folder or its marker) or *File > Refresh all dependencies* brings the new
version in again.

*File > Refresh all dependencies* re-reads every library and data file, even
ones whose timestamp did not change.

## Opening a design whose libraries changed

A design remembers which version of each library interface it was wired
against. When you open it after a library changed, your wiring is updated the
same way a refresh would update it, and a message says what happened (with
*Details* when something was disconnected). If only the library's internals
changed, you are told that it *changed since this file was last saved*.

## Changing the file and unlinking

*Change file…* on a library folder points the alias at another file — typically
a newer version (`demolib_v3.cnnd` → `demolib_v4.cnnd`). It works like a
refresh: one undo step, wires follow parameters by identity, and the report
says what changed. Nothing in your design is renamed.

*Unlink* removes a library you no longer use. It is refused, with the list of
nodes that still use the library, while anything does — so unlinking can never
break a working node. Unlinking is one undo step.

## Renaming an alias

*Rename alias…* on a library folder moves the library to another alias —
`demolib` to `libs.demolib`, say — together with **every** use of it in your
design: custom nodes, record nodes, types that name one of its record types,
nodes that refer to something the library no longer defines, and the libraries
it links (`demolib.common` becomes `libs.demolib.common`). Wires are not
touched, and the library file is not changed; the next save writes the new
alias. It is one undo step.

The new alias follows the same rules as a new link's. Two more are specific to
a rename: it may not contain or lie inside the current alias (go through
another name — `demolib` → `tmp` → `demolib.v2`), and it is refused when
something in your design already refers to a name under it that does not exist,
since that reference would suddenly start pointing into the library.

## Making a library local

*Make local copy* on a library folder ends the link and keeps the content: the
library's networks, record types and folders — and those of the libraries it
links — become part of your design, exactly as they are in memory now (also if
the file on disk has changed since). From then on they are editable, saved in
your design, and changes to the library file no longer reach it; the design no
longer lists the library. Their names do not change (`demolib.half_space` stays
`demolib.half_space`, now as an ordinary folder of your own), so every wire
stays where it is. Use it when one self-contained file is what you need, or to
take over a library you want to develop further inside your design.

Data files the library reads by a relative path (an `import_xyz` of `tip.xyz`
beside the library, say) keep being read from the same place: the stored paths
are rewritten relative to your design's folder (`libs/tip.xyz`). A path that
arrives through a wire is computed during evaluation and cannot be rewritten.

It is offered only while the library is loaded, and refused while something
refers to a name the library does not define (a node showing *Unknown node
type*) — outside a library such a node would lose its wires. It is one undo
step; undo makes it a link again.

## From the command line

A running atomCAD can be driven by
[`atomcad-cli libraries`](headless_cli.md#linked-libraries-atomcad-cli-libraries):
list, link, refresh, unlink, rename an alias, and make a library local.
`atomcad-cli query` marks a network that belongs to a linked library with a
`# linked from <path>` line under its header: it can be read, but an edit of it
is refused.

## Moving a design: Save As and project bundles

A design and the files it depends on — its linked libraries, the libraries they
link, and the data files read by its nodes and by the libraries' nodes (`.xyz`,
`.cif`, `.cube`, operation libraries, build scripts) — form a fixed layout of
relative paths. Paths are never rewritten, so moving the design means moving
that layout with it.

**Save Design As** into another folder checks every dependency first. If each
one is already where the design will look for it (for example, you saved from
`proj1/` into a sibling `proj2/` and both use `../libs/`), the design is simply
saved. Otherwise a dialog lists them in three groups:

- **Inside the destination folder** — shown by their relative path;
- **Outside the destination folder** — reached through `..`, shown with the
  full path they would be copied to, since those copies land outside the folder
  you picked;
- **External** — data files referred to by an absolute path; never copied, the
  saved design keeps pointing at them.

Each entry says *will copy*, *already there* (the same file, or one with
identical content — skipped), *different file already there*, or *missing now
too*. The buttons:

- **Copy dependencies** copies what is needed and then saves the design. If a
  different file is already at a target, you must choose between overwriting it
  and keeping it; keeping it means the saved design will use that other
  version.
- **Save without dependencies** writes only the design; hover it to see how
  many libraries and data files will be missing at the new location. A missing
  library loses nothing: its nodes keep their wires and show errors until the
  file is there.
- **Cancel**.

Dependencies are copied before the design is written. If a copy fails, the
design is not saved, and the message lists the copies already made (they are
left in place). A copy never lands on the design file itself, on one of its
other dependencies, on a folder, or through a link — Save As to such a place is
refused before anything is written.

A node whose file path arrives through a wire is only known when the network
runs; the dialog warns when the design has one, since that file is not copied.

After saving, the open design reads its libraries and data files from the new
location: a copy is used silently; a different file you chose to keep is
brought in like any other change (one undo step, with a report).

**File > Export project bundle…** writes a `.zip` of the design — as it is now,
unsaved changes included — and every dependency, with their paths relative to
the smallest folder containing them all. Unzip it anywhere and the design opens
with all its libraries. External and missing files are left out, and the
message lists them. This is the way to send a design to someone else.
