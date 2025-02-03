Node networks are like function definitions. The node network is just a superficially different representation than a textual function definition. There a few small differences. The node network can have some nodes, which just hang there, these are calculated values, which might be useful sometime.

This is a node network without parameter and without return value:

```
                  [sphere<2>]---->|---------|     
                                  |intersect|-->VALUE1
[cuboid<10,10,10>]-->|----|   +-->|---------|     
                     |diff|---+                   
[cuboid<5,5,5>]----->|----|   +-->|---------|     
                                  |intersect|-->VALUE2
                  [sphere<3>]---->|---------|     
```

The above node network is equivalent with this code:

```
let TMP = diff(cuboid<10,10,10>(),cuboid<5,5,5>());
let VALUE1 = intersect(sphere<2>,tmp);
let VALUE2 = intersect(tmp,sphere<2>);
```

VALUE1 and VALUE2 are not part of the network, I just labeled those node's outputs. The node network in my mind is the same as a textual mathematical expression, the only difference is that it is just more expressive than a one line textual mathematical expressions because in textual expressions a function can be fed into only one other function (one output wire), but here it can have multiple output wires (DAG instead of tree). In text this needs to be dealt with using a 'let' and a temporal variable notation (see tmp). Other than this surface level change I do not see any difference.

The above code can be made a function definition with parameters. Correspondingly the network can have parameters, which are special nodes. If you place a special parameter node in a network, that will become one of its parameters. Functions also need an output node. I made the above network a function:



```
                  [sphere<2>]------>|---------|     
                                    |intersect|---+
[input<1,"arg1">]----->|----|   +-->|---------|   |
                       |diff|---+                 |-->[output] 
[input<2,"arg2">]----->|----|   +-->|---------|   | 
                                    |intersect|---|
                  [sphere<3>]------>|---------|     
```

Equivalent code:

```
fn mynetwork(arg1, arg2) {
  let tmp = diff(arg1, arg2);
  return union(intersect(sphere<2>,tmp),intersect(tmp,sphere<2>));
}
```

As a function can be 'called' in another function, a network can be referred to by name in another network as node.

```
                  
[cuboid<10,10,10>]-->|---------|     
                     |mynetwork|--->VALUE                  
[cuboid<5,5,5>]----->|---------|     

```

Equivalent with:

```
let VALUE = mynetwork(cuboid<10,10,10>, [cuboid<5,5,5>]);
```

