- add crystal site type property to atoms (0 and 1)
- for zincblende only allow stamp placement on a site which has the same site type as the anchor atom. For non zincblende allow the placement, but signal on the user interface that the enantiomer is placed. 
- when evaluating a stamp placement check if the site type of the anchor atom is the same as the site type of the placement,
and if they are different, multiply the inversion matrix with the rotation matrix. (used for non zinc blende).
- support deleted atoms
- support defect bond placement and deletions
- support geo_trans correctly
- support atom_trans correctly
