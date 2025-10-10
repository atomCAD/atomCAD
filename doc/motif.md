To enable a composable way to create crystal structures it seems that we need 3 quasi orthogonal and reusable concepts (data types) which can be combined together (with some additional parameterization) in the geo_to_atom node (which should be probably renamed to atomic_fill or something similar). The 3 concepts (data types) are the following:

* Unit cell (just the 3 basis vectors)
* Geometry
* Motif

The first two are already well established in atomCAD, so we need to define what a motif is.

It is a tempting design decision to just not introduce a new concept and try to handle the motif as any atomic structure. The advantage of this decision would be that these would be editable as any other atomic structure. The drawback is that motifs are basically templates and need features which makes them much more abstract than a simple atomic structure.

I think in the long term we should support the conversion of regular atomic structures to motifs by using some additional data, but in the short term we need a quick way to define motifs which are generic enough for composable design, and this quick way is a simple but relatively powerful and extendable motif definition language.

## Motif definition language in atomCAD

We introduce a *motif* node which will have a text area on its UI and the motif can be specified as a text in atomCAD’s motif definition language.

The requirements of the language are basically parameterized fractional atom sites, explicit & periodic bond definitions.


There are 3 commands in the language for now: `param`, `site` and `bond`

The `param` command simply defines a *parameter element*. The name of the parameter element needs to be specified followed optionally by the default element name. Example parameter elements in the zincblende motif:


```
param PRIMARY C
param SECONDARY C
```


Parameter elements are the ones that are replaced by concrete elements which the user chooses in the `geo_to_atom` node.


The `site` command defines an atomic site. You need to specify the site id, an element name, (which can be a regular element name like `C` or a parameter element).Then the 3 fractional lattice coordinates need to be specified. (Fractional coordinates are always 0 to 1. The unit cell basis vectors will be used to convert these to real cartesian coordinates.)


Examples:

```
site 1 PRIMARY x y z
site 2 SECONDARY x y z
```


Finally the bond command defines a bond. Its two parameters are *site specifiers*. A site specifier is a site id optionally prefixed by a 3 character relative cell specifier. The relative cell specifier's three characters are for the three lattice directions: '-' means shift backwards in the specific direction, '+' means shift forward, '.' means no shift in the given direction.

Example:


```
bond +..1 2
```

The above means that there is a bond between the site with id `1` in the neighbouring cell in the +x direction and the site with id `2` in this cell.

Comments are supported with starting a line with a `#` character.
Comment lines as well as empty lines (containing only whitespaces) are
accepted and ignored by the parser.
