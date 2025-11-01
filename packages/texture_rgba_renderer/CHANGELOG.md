## 0.0.16
* opt: remove unneeded len check on Linux.

## 0.0.15
* opt: add the parameter check for len,width,height on Linux. 

## 0.0.14
* fix: remove the `copy_pixels` function pointer initialization in texture rgba class.

## 0.0.13
* try to fix the memory leak on macOS.

## 0.0.12
* add `stride_align`.

## 0.0.11
* compatible for ubuntu 18.04 toolchain.

## 0.0.10
* try fixing the memory corrupt issue on Linux. (change to pixel texture)

## 0.0.9
* try fixing the memory corrupt issue on Linux.

## 0.0.7
* add the FFI function and ptr getter for macOS.

## 0.0.6
* Please use `BGRA` format instead.
* Memory leak fix for Linux.

## 0.0.5
* Initial macOS support.

## 0.0.4
* Fix windows memory corrupt.

## 0.0.3
* Initial pixel based rgba render for Windows. Note that we use the buffer directly without copy, which may have some memory corrupt.

## 0.0.2

* Initial Hardware accelerated rgba render for Linux.
