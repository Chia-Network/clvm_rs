use std::cell::{RefCell, RefMut};

use pyo3::prelude::pyclass;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyBytes, PyTuple};

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;

use super::clvm_object::CLVMObject;
use super::py_view::PyView;

#[pyclass(subclass, unsendable)]
pub struct PyArena {
    arena: RefCell<IntAllocator>,
    cache: PyObject,
}

impl PyArena {
    pub fn new(py: Python) -> PyResult<&PyCell<Self>> {
        PyCell::new(
            py,
            PyArena {
                arena: RefCell::new(IntAllocator::default()),
                cache: py.eval("dict()", None, None)?.to_object(py),
            },
        )
    }

    pub fn allocator(&self) -> RefMut<IntAllocator> {
        self.arena.borrow_mut()
    }

    pub fn add(
        &self,
        py: Python,
        obj: &PyCell<CLVMObject>,
        ptr: &<IntAllocator as Allocator>::Ptr,
    ) -> PyResult<()> {
        let locals = [
            ("cache", self.cache.clone()),
            ("obj", obj.to_object(py)),
            ("ptr", ptr.to_object(py)),
        ]
        .into_py_dict(py);

        py.run("cache[ptr] = obj; cache[id(obj)] = ptr", None, Some(locals))
    }

    // py to native methods

    fn from_py_to_native_cache<'p>(
        &self,
        py: Python<'p>,
        obj: &PyCell<CLVMObject>,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        let locals = [("cache", self.cache.clone()), ("key", obj.to_object(py))].into_py_dict(py);
        py.eval("cache.get(id(key))", None, Some(locals))?.extract()
    }

    fn populate_native(
        &self,
        py: Python,
        obj: &PyCell<CLVMObject>,
        allocator: &mut IntAllocator,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        apply_to_tree(obj.to_object(py), move |obj| {
            let node: &PyCell<CLVMObject> = obj.extract(py)?;

            // is it in cache yet?
            if self.from_py_to_native_cache(py, node).is_ok() {
                // yep, we're done
                return Ok(None);
            }

            // it's not in the cache

            match &node.borrow().py_view {
                PyView::Atom(obj) => {
                    let blob: &[u8] = obj.extract(py).unwrap();
                    let ptr = allocator.new_atom(blob).unwrap();
                    self.add(py, node, &ptr)?;

                    Ok(None)
                }
                PyView::Pair(pair) => {
                    let pair: &PyAny = pair.clone().into_ref(py);
                    let pair: &PyTuple = pair.extract()?;
                    let p0: &PyCell<CLVMObject> = pair.get_item(0).extract()?;
                    let p1: &PyCell<CLVMObject> = pair.get_item(1).extract()?;
                    let ptr_0: PyResult<i32> = self.from_py_to_native_cache(py, p0);
                    let ptr_1: PyResult<i32> = self.from_py_to_native_cache(py, p1);
                    if let (Ok(ptr_0), Ok(ptr_1)) = (ptr_0, ptr_1) {
                        let ptr = allocator.new_pair(ptr_0, ptr_1).unwrap();
                        self.add(py, node, &ptr)?;
                        Ok(None)
                    } else {
                        Ok(Some((p0.to_object(py), p1.to_object(py))))
                    }
                }
            }
        })?;

        self.from_py_to_native_cache(py, obj)
    }

    pub fn native_for_py(
        &self,
        py: Python,
        obj: &PyCell<CLVMObject>,
        allocator: &mut IntAllocator,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        self.from_py_to_native_cache(py, obj)
            .or_else(|_err| self.populate_native(py, obj, allocator))
    }

    // native to py methods

    fn from_native_to_py_cache<'p>(
        &self,
        py: Python<'p>,
        ptr: &<IntAllocator as Allocator>::Ptr,
    ) -> PyResult<&'p PyCell<CLVMObject>> {
        let locals = [("cache", self.cache.clone()), ("key", ptr.to_object(py))].into_py_dict(py);
        py.eval("cache[key]", None, Some(locals))?.extract()
    }

    fn populate_python<'p>(
        &self,
        py: Python<'p>,
        ptr: &<IntAllocator as Allocator>::Ptr,
        allocator: &mut IntAllocator,
    ) -> PyResult<&'p PyCell<CLVMObject>> {
        apply_to_tree(*ptr, move |ptr| {
            // is it in cache yet?
            if self.from_native_to_py_cache(py, &ptr).is_ok() {
                // yep, we're done
                return Ok(None);
            }

            // it's not in the cache

            match allocator.sexp(&ptr) {
                SExp::Atom(a) => {
                    // it's an atom, so we just populate cache directly
                    let blob = allocator.buf(&a);
                    let py_bytes = PyBytes::new(py, blob);
                    self.add(
                        py,
                        CLVMObject::new(py, PyView::new_atom(py, py_bytes))?,
                        &ptr,
                    )?;
                    Ok(None)
                }
                SExp::Pair(ptr_1, ptr_2) => {
                    // we can only create this if the children are in the cache
                    // Let's fine out
                    let locals = [
                        ("cache", self.cache.clone()),
                        ("p1", ptr_1.to_object(py)),
                        ("p2", ptr_2.to_object(py)),
                    ]
                    .into_py_dict(py);

                    let pair: PyResult<&PyAny> =
                        py.eval("(cache[p1], cache[p2])", None, Some(locals));

                    match pair {
                        // the children aren't in the cache, keep drilling down
                        Err(_) => Ok(Some((ptr_1, ptr_2))),

                        // the children are in the cache, create new node & populate cache with it
                        Ok(tuple) => {
                            let (p1, p2): (&PyCell<CLVMObject>, &PyCell<CLVMObject>) =
                                tuple.extract()?;
                            self.add(
                                py,
                                CLVMObject::new(
                                    py,
                                    PyView::new_pair(py, PyTuple::new(py, &[p1, p2]))?,
                                )?,
                                &ptr,
                            )?;
                            Ok(None)
                        }
                    }
                }
            }
        })?;

        self.from_native_to_py_cache(py, &ptr)
    }

    pub fn py_for_native<'p>(
        &self,
        py: Python<'p>,
        ptr: &<IntAllocator as Allocator>::Ptr,
        allocator: &mut IntAllocator,
    ) -> PyResult<&'p PyCell<CLVMObject>> {
        self.from_native_to_py_cache(py, ptr)
            .or_else(|_err| self.populate_python(py, ptr, allocator))
    }
}

fn apply_to_tree<T, F>(node: T, mut apply: F) -> PyResult<()>
where
    F: FnMut(T) -> PyResult<Option<(T, T)>>,
    T: Clone,
{
    let mut items = vec![node];
    loop {
        let t = items.pop();
        if let Some(obj) = t {
            if let Some((p0, p1)) = apply(obj.clone())? {
                items.push(obj);
                items.push(p0);
                items.push(p1);
            }
        } else {
            break;
        }
    }
    Ok(())
}
