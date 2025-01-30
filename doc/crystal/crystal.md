# AtomCAD Model Representation and Kernel Architecture 

## Model representation

The design philosophy behind the concept described in this document is the following:

- We would like to use non-destructive editing where possible
- We would like to enable structure/code reuse as much as possible. The user should be able to distill useful parts of the design into a reusable representation. In the long run the possibility of an ecosystem of part libraries should be achievable.

I propose that an atomCAD model representation consist of 2 relatively well separated things:

- **SDF function libraries**
- and **atomic entities** where an atomic entity is a stack of operations.

### Overview

* About SDF: We plan to use SDF (Signed Distance Field) to express solid geometry in atomCAD. SDF functions is a very elegant and composable way to achieve interesting solid geometries. In atomCAD we have a strong need for supporting CSG (Constructive Solid Geometry) from primitives like cutter planes with specified Miller indices along crystal lattice points. Fortunately CSG is just a special use-case for SDFs: unions and intersections being min and max functions in an SDF.

- Defining a geometry as an SDF can be already seen as non-destructive editing. In atomCAD generally we would like to support non-destructive operations where possible even outside of SDFs too. For this reason I propose that an atomic entity is a stack of operations. This proposed operation stack is very similar to the modifier stack in Blender. (The term 'operation' is used instead of 'modifier' because even the first operation in the stack is not special.) Most of the time the first operation in the stack will be the 'build atoms from SDF' operation which builds an atomic representation from an SDF geometry non-destructively using plane and edge fixes. Non-destructive means that you can change the SDF anytime, and the resulting built atomic representation will change implicitly too.

- We would like to support structure reuse where possible. We support the creation of SDF function libraries. An entity does not define its SDF geometry: It just refers to an SDF function in a 'build atoms from SDF' operation.
- We might find that in some cases we do not find an elegant way to support some operations non-destructively. The user might want to do atom-by-atom operations. We will support this, but we record these direct operations into the operation stack of the entity too. Replaying these modifications upon significant SDF changes can make these modifications non-applicable. We will work on ways to make these cases as rare as possible, but in these cases we will warn the user that some steps will be lost if they proceed. ('Lost' in the sense of not applicable for the new entity. Ideally nothing should be lost forever, the questions is just on what level is version control achieved. We will discuss version control later.) 

### Why SDF functions + operation stacks?

When designing non-destructive representations, there is a continuum of ways to arrange operations from the less powerful but easily understandable and managable to the very powerful and procedural but more complicated ones. In the order of expressing power:

- a stack of operations
- a tree of operations
- a graph of operations (a. k. a. node graph)
- a general Turing complete programming language

At first I thought that a tree of operations would be a good compromise for the whole atomCAD, but it turned out that:

- It might bee too restrictive to define interesting geometries elegantly. I prefer the full power of a programming language when expressing an SDF.
- On the other hand a tree is too cumbersome in most other cases. Most of the time operations with only one argument would result in unnecessarily deep trees where a stack of operations would suffice. 

In the future we will probably support the full programming language approach for entities beyond SDFs too (to procedurally create *anything* in atomCAD), but I think we need the operator stack approach there for ease of editability in most cases for most users.

### An example

Let's see a concrete example. The user wants to create an atomic gear. Chances are that this problem has been solved before, and indeed they find an SDF function in their function library with the following signature:

`double generic_gear(Vec3 pos, int radius, int width, int num_of_teeth);`

The user creates their own SDF function:

```
double my_gear(Vec3 pos) {
	return generic_gear(pos, 10, 2, 12);
}
```

The user then creates a new empty atomic entity. Then presses the 'add operation' button beside the entity and choses to add a 'Build atoms from SDF' operator. The user can set the operator parameters on the screen which is the name of the SDF function to use and parameters related to surface fixing. Choses the `my_gear` SDF function and accepts the default options for surface fixing and adds the operation. The entity now has one operator on its operator stack. The gear appears in the viewport with 12 teeth.

Now the user goes back to edit the SDF function `my_gear`, and changes it to:

```
double my_gear(Vec3 pos) {
	return generic_gear(pos, 10, 2, 16);
}
```

The gear is refreshed to have 16 teeth. The user might even click on the gear parameters and use a slider to change them and experiment with them.

## SDF functions in atomCAD

In its simplest form an SDF is simply a function that takes a 3 dimensional vector as a parameter and produces a floating point number:

`double my_sdf(Vec3 pos);`

It is easy to see how such an SDF function can be meaningfully reused in another SDF function. We might want to instantiate the shape defined in my_sdf inside my_other_sdf but translated along a translation vector of (1,2,3). This can be achieved this way:

```
double my_second_sdf(Vec3 pos) {
	return my_sdf(pos - Vec3(1,2,3));
}
```

The 'build atoms from SDF' operation can refer to a function with the above defined signature. Of course we can create reusable functions that have different signatures. Some functions can have other parameters beside the `pos` parameter, these can be thought of as parametric parts. Other functions can be just helper functions with totally different signatures.

When it comes to the definition of an SDF the question arises whether to create a special language for defining SDFs or support writing SDF-s in an existing general-purpose programming language?

- A special language is probably required if we would like to support a node based UX for editing the SDF functions instead of textual editing. We will review the UX aspect soon.

- A special language needs to be created if there are requirements for the language that existing languages do not conveniently support. We will review this aspect too.

Please note that a hybrid approach is also possible: define most of the SDF in a special language but also provide a way to create 'custom nodes' in a general purpose programming language for the very complicated cases.

### UX considerations

Bret Victor made a demonstration of a system in which meaningful parameters could be intuitively set despite the textual programming language. It is in the first 10 minutes of his talk called "Inventing on Principle":

https://www.youtube.com/watch?v=EGqwXt90ZqA

As seen in the talk it is enormously valuable if the change you make to the code is displayed immediately on the screen, so very quick edit-to-run time is a language requirement.

Beside the immediate feedback we should think about how can we make the editor smart enough.

For certain functions clicking on a function call with constant numbers as parameters, an editing functionality might be activated in the viewport, depending on a function. For example for half-spaces the editing apparatus should come up to define a plane. Even if no such function exist for a parameter a generic approach can be used as In Bret Victor's demonstration.

Detecting function calls with constant parameters in the editor is trivial even without using an AST (abstract syntax tree), so this do not pose a special requirement for the language.

Currently I do not see a very big advantage of having a node graph instead of a textual language with smart editor features, so this alone should not be a requirement for a special language in my opinion.

### Language requirements

In the UX section we gathered one requirement: Edit-to-run times should be very small. Please note that there are two edit-to-run times to consider:

- We can be more tolerant with the time after a generic edit 
- Where we should be really fast is when the user changes a constant parameter of a function.

Other language requirements are related to sandboxing. We need a language that has reasonable sandboxing.

Language requirements can come from the perspective of evaluation of the SDF functions. Is there a special way these functions should be evaluated? I see one thing that can be useful: to calculate the gradient at a point. This can be done using numerical differentiation, but it is expensive (taking multiple samples) and can be inaccurate. I think we can live with numeric derivation but automatic derivation would be a big plus. Automatic derivation can be achieved even in an existing language, but it is very inconvenient and ugly if the language do not support operator overloading.

Another requirement is the need for knowing the dependency tree between functions. When editing a reusable function, we need to know which atomic entities to refresh. I think this can be done relatively easily in any language.

Summary: We need an easily embeddable language with reasonable sandboxing, and very quick edit-to-run numbers. We need to know the dependency tree between functions. If the language provides operator overloading (to help automatic differentiation) it is a big plus, but probably not a necessity, as we can live with numeric differentiation.

Please note that fortunately the choice of programming language is not a decision that the whole architecture depends on: we can change the language without changing most other parts of the system. The surface area between the SDF and the other parts is just an evaluation function.

#### Initial choice for the programming language

As the Kernel is written in Rust we need a language that is easily integrated into Rust.

A promising choice seem to be https://rhai.rs/

Supports operator overloading. It is interpreted, so there is no compilation: edit to run delay is probably minimal. On the other hand it might be a bit slow due to being interpreted.

#### Possible problem with intuitive editing

There can be a problem with an intuitive editing workflow combined with a completely generic SDF. The problem is that even if we provide an intuitive way to edit something (like edit  plane intuitively), in SDF there can be a function applied to it which transforms it completely. So the user will not directly edit the final shape in this case. (But at least the final shape is also displayed as an immediate feedback). This cannot be completely avoided, but a partial solution is that users can edit a reusable SDF function and then use its transformed version in another function. When editing the reusable function (the 'component', or the 'part'), the user edits in-place.



 ### 'Build atoms from SDF' operator

This operator creates an atomic entity from an SDF. The operator can have several parameters for example regarding how to fix certain planes and edges. Please note that at SDF evaluation time there is no explicit notion of planes and edges: We should work from whatever evaluations we do in the SDF (value evaluations and gradient evaluations). Plane and edge fixing algorithms can use SDF values and gradients at certain atoms and other crystal points for their heuristics. 

## Version control

What would be an undo history in a direct editing program is already embedded into the representation itself (in the form of non-destructible operations both in the SDF functions as function calls and in atomic entities as operators on the stack), so changes in our representation (like SDF function source code changes) are 'meta changes' in this sense. Any version control on a representation which itself contains 'operation history' in itself seem to be a bit redundant, but still a necessity mainly because we want the users not to lose previous versions of their model.

### Simple and crude version control

We should probably develop our version control on top of our representation in the long term. In the short term a simple and crude method would be to store a model as a sensible file hierarchy and let `git` do the version control work. This is much better than nothing but has drawbacks. The main drawback is that git cannot interpret and handle diffs in binary files. If we represent even atomic operators as well-designed textual files (designed with diffing in mind) or at least separate each operation into a separate file, we can achieve better diffs.

TODO: The topic of version control needs to be investigated in more detail. 
