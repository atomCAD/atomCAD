# Creating a mode advanced type system in the atomCAD node network

This desigo document is created to make changes to the atomCAD design starting at Sepetember 18, 2025. The document is not meant to be maintained indefinitely.  

## APIDataType

```rust
pub enum APIDataType {
 None,
 Bool,
 String,
 Int,
 Float,
 Vec2,
 Vec3,
 IVec2,
 IVec3,
 Geometry2D,
 Geometry,
 Atomic
}

```

`DataType` and `APIDataType` might be different?

What is `APIDataType` used for on the API?

```rust
pub struct APIParameterData {
  pub param_index: usize,
  pub param_name: String,
  pub data_type: APIDataType,
  pub multi: bool,
  pub sort_order: i32,
}

pub struct APIExprParameter {
  pub name: String,
  pub data_type: APIDataType,
}

pub struct APIExprData {
  pub parameters: Vec<APIExprParameter>,
  pub expression: String,
  pub error: Option<String>,
  pub output_type: Option<APIDataType>,
}

// Only used for the API type dropdown menu
pub fn get_api_data_type_display_name(data_type: APIDataType)
```



This how the data type drop down menu is used on the UI: 

```dart
items: APIDataType.values.map((dataType) {
      return DropdownMenuItem(
        value: dataType,
        child: Text(getApiDataTypeDisplayName(dataType: dataType)),
       );
      }).toList()
```



So APIDataType on the Flutter side is used to select the type of parameters, expression parameters and expression return types.



## What is needed on the UI?

In the long term a complex way to construct any type.

In the short term a way to select simple types using a dropdown and an array checkbox, or entering more complex types using plain text.

We concentrate for the short term.

We need the following structs on the API:

```rust
pub enum APIBuiltInDataType {
 None,
 Bool,
 String,
 Int,
 Float,
 Vec2,
 Vec3,
 IVec2,
 IVec3,
 Geometry2D,
 Geometry,
 Atomic
}

pub enum APIDataType {
  BuiltIn {
    data_type: BuiltInDataType,
    array: bool,
  },
  Custom(String),
} 
```

We will also create a common reusable data type entry widget.

## DataType

In Rust the core we will use a different representation, which is named `DataType`.

```rust

struct FunctionType {
   parameter_types: Vec<DataType>,
   output_type: Box<DataType>,  
}

enum DataType {
 None,
 Bool,
 String,
 Int,
 Float,
 Vec2,
 Vec3,
 IVec2,
 IVec3,
 Geometry2D,
 Geometry,
 Atomic,
 Array(Box<DataType>),
 Function(FunctionTzype),
}
```

## Arrays

For any input pin, which has an `Array<T>` type the system allows the user to connect multiple output pins of the `Array<T>` or `T` types. These are appended together into an array. The order is not determined.

## NetworkResult modification

At runtime the array type is represented as a `Vec<NetworkResult>`. A functions is represented simply as a node type name.

## Incremental development plan

### First phase: existing functionality with the new types

In the first phase we introduce `DataType`, `APIDataType` and `APIBuiltInDataType`. We introduce the widget on the UI to edit a data type.

We also introduce the new handling of arrays. This is already quite a big commit, but in this we do not implement any higher order function and fake function data type. The aim is a still working application but with the new data types.

### Second phase: functions as data types, higher order functions

In this phase we support function typed outputs and inputs and make the first higher order function work: map.



