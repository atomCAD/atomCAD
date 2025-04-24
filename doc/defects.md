# Defects

This document is about the representation and UX of editing defects in atomCAD Structure Designer.

## Aim of Structure Designer in atomCAD

The aim of the Structure Designer is to enable users to easily and non-destructively edit arbitrarily complex crystal structures without repetitive work.

## Representation

In Structure Designer a crystal should be composed independently of its bounding geometry and its defect pattern.

`Crystal = Bounding Geometry + Defect pattern`

These two can be edited independently, thus the editing experience is non-destructive. (When you change you bounding geometry, your defect patterns are applied meaningfully.)

What is a bounding geometry? It is a function:

`Bounding Geometry = F(float x, float y, float z) -> float. Positive: inside, negative oustside (Implicit geometry (sub category: Signed Distance Field))`

What is a defect pattern? IT is a function too:

`Defect pattern = D(int ux, int uz, int uz, int ui)` -> Element

The input of this function is the address of an atom in the crystal structure: integer unit cell coordinates, and atom index inside the unit cell.

The output is what element should be there. It can be Carbon (the default), it can be 'Deleted', or any other element (replacement).

Similarly to geometry, Defects can be possibly constructed of multiple nodes.

We will have a default common defect node, in which:

- you can have defects in a cubic area
- you can repeat this in x, y, z directions in finite counts or infinitely.

## Scripting

We should support the user to be able to define a defect pattern function programmatically (this is also true for geometries).

Our kernel is written in Rust, so we can easily allow users to write function in Rhai, which is an embedded scripting language for Rust:

https://github.com/rhaiscript/rhai

## UX

Editing the Defects should be straightfoward: click on atom and being able to delete or replace it.

The harder thing is how to show 3D volumetric data to the user.

I think one of the most useful UX is to use axis aligned cross section planes.

