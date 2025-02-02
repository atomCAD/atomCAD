# Shape algebra

## Introduction, atomCAD context

In atomCAD we would like to define a formal language to represent atomic structures. Having this formal language we can easily serialize into a textual file format.

An atomCAD atomic sturcutre is a tree of operators. To give users the power of a node network UX, an atomCAD document is an extension of this: a DAG (Directed Acyclic Graph) of operators.

An operator has:

- some number of inputs: 0 (literal), 1 (unary), 2 (binary)
- an output
- and some parameters, which are different for each kind of operator and defined by the user. Example: the half_space operator (a.k.a cutter plane) has for numbers as parameters: its miller index and an offset.

In this document I want to examine a subset of these operators that only deal with the geometry, so this document does not address operators dealing with the atomic representation.

## Basic set of operators

To get started first I define a basic set of operators we will definitely need.

I use the following format:

`operator_name<parameters>(inputs)`

If an operator do not have parameters I omit the `<` and `>` signs, and if the operator do not have inputs I omit the `(` and `)` signs.

Each operator has one output. Inputs and outputs of the operators are 3D shapes (infinite sets of points in 3D space).

Parameters are numbers or vectors of numbers.

Basic operators:

```
half_space<miller_index, offset>() // a.k.a. cutter plane
union(a, b)
intersection(a, b)
neg(a)
diff(a, b)
```

The meaning of these operator is self explanatory.

## Abstraction

Please note that the above operators are meant in a purely abstract way: as an algebra upon 3D shapes (infinite point sets).

We examine a tree in the above language and it we will know exactly what infinite set of 3D points it refers to without knowing how it is treated in a computer. 

There are different *implementing models* that can implement such a shape algebra under the hood. Perhaps the word *implement* is not accurate as we are still talking about mathematical concepts and not concrete programs in concrete programming languages, but I use this word as nothing better comes to mind.

## Implementing models

I list here 3 implementing models for the above algebra. Each implementing model can be used on a computer to generate displayable representation on a computer, or in our case decide which atoms in an infinite crystal lattice are inside the shape.

### Implicits

In this implementation for each operator we give an implicit function. The implicit function's parameters are the position in space and the parameters and inputs of the operator, the output is  bigger than zero if the point is out of the shape and smaller than or equal to zero if in the shape. Deciding whether a point is in the space can be done by calling the functions of the tree recursively. An implicit function for all the above defined operators is very simple.   

### Polygon mesh (approximation)

Any shape we are interested in can be approximated by a polygon mesh. It is easy to implement all the above operators on polygon meshes. (This is an approximation though for curved shapes.) With the above defined basic operators only polygonal meshes can be created, so the model is precise for this subset. The only special treatment is that we cannot represent infinite planes as a polygon, so the graph needs to be constructed in a way that all nodes except the leaf nodes represent shapes bounded by a finite surface. 

### Voxel approximation

In this case we can approximate any shape with a voxel: this can support any shape but obviously just as an approximation with accuracy vs. memory consumption tradeoff.

## Value of implementation independence

The main question of this document is to what extent can we extend this language to support some common needs of the user, but still maintain the status that we can change the underlying implementation under the hood? Do we need to make our representation dependent on the underlying implementing model, or can this representation remain abstract in this way?

This is an important question because keeping the implementation independence can be important for the following reason:

- Derisk the development. If we operate with an implementation and it turns out that we need a new operation which is very problematic with that implementation, we can still change implementation at that point without breaking our core representation and file format.

## Procedural generation of an operator tree

We know that a very complicated procedural shape can be achieved if we stick with the implicit implementation by writing a complex implicit function in a general purpose programming language. Can proceduralism be used independently of the underlying implementation? For most of the cases yes: A technique can be used where a script generates an operator tree procedurally. This can be used for lots of things, like a gear with N teeth, etc. 

## Procedural Extensions that keep implementation independence

If we do not want to write scripts, but still have procedural capabilities, we can extend the language itself with operators that support this. The simplest and most useful of this kind is a **pattern** operator used in many CAD systems which generates copies of its argument shape in a pattern defined by its parameters.

The pattern operator can be implemented a way as macro: internally it expands into lots of operators in the operator DAG.

We might be able to implement other operators which are macros.

In other cases we might just support a new operation in our underlying model implementation (e.g. support it in implicits). 