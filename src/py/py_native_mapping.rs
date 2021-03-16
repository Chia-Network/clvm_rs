use pyo3::prelude::*;
//use pyo3::pyclass::PyClassAlloc;`
use pyo3::types::{IntoPyDict, PyBytes, PyTuple, PyType};

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;

use super::native_view::NativeView;
use super::py_int_allocator::PyIntAllocator;
use super::py_view::PyView;

use super::py_na_node::PyNaNode;

pub struct PyNativeMapping {
    native_to_py: PyObject,
}

impl PyNativeMapping {
    pub fn new(py: Python) -> PyResult<Self> {
        let native_to_py = py
            //.eval("__import__('weakref').WeakValueDictionary()", None, None)?
            .eval("dict()", None, None)?
            .to_object(py);
        Ok(Self { native_to_py })
    }

    fn add(
        &self,
        py: Python,
        obj: &PyCell<PyNaNode>,
        ptr: &<IntAllocator as Allocator>::Ptr,
    ) -> PyResult<()> {
        obj.borrow().int_cache.set(Some(ptr.clone()));

        let locals = [
            ("native_to_py", self.native_to_py.clone()),
            ("obj", obj.to_object(py)),
            ("ptr", ptr.to_object(py)),
        ]
        .into_py_dict(py);

        let r = py.run("native_to_py[ptr] = obj", None, Some(locals));
        r
    }

    // py to native methods

    fn from_py_to_native_cache<'p>(
        &'p self,
        py: Python<'p>,
        obj: &PyCell<PyNaNode>,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        let ptr: Option<i32> = obj.borrow().int_cache.get();
        let locals = [
            ("cache", self.native_to_py.clone()),
            ("key", ptr.to_object(py)),
        ]
        .into_py_dict(py);
        let obj1: &PyCell<PyNaNode> = py.eval("cache[key]", None, Some(locals))?.extract()?;
        if obj1.to_object(py) == obj.to_object(py) {
            Ok(ptr.unwrap())
        } else {
            py_raise(py, "not in native cache")
        }
    }

    fn populate_native(
        &self,
        py: Python,
        obj: &PyCell<PyNaNode>,
        allocator: &mut IntAllocator,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        apply_to_tree(obj.to_object(py), move |obj| {
            let node: &PyCell<PyNaNode> = obj.extract(py)?;

            // is it in cache yet?
            if self.from_py_to_native_cache(py, node).is_ok() {
                // yep, we're done
                return Ok(None);
            }

            // it's not in the cache

            match &node.borrow().py_view {
                Some(PyView::Atom(obj)) => {
                    let blob: &[u8] = obj.extract(py).unwrap();
                    let ptr = allocator.new_atom(blob).unwrap();
                    self.add(py, node, &ptr)?;

                    Ok(None)
                }
                Some(PyView::Pair(pair)) => {
                    let pair: &PyAny = pair.clone().into_ref(py);
                    let pair: &PyTuple = pair.extract()?;
                    let p0: &PyCell<PyNaNode> = pair.get_item(0).extract()?;
                    let p1: &PyCell<PyNaNode> = pair.get_item(1).extract()?;
                    let ptr_0 = &p0.borrow().int_cache.get();
                    let ptr_1 = &p1.borrow().int_cache.get();
                    if let (Some(ptr_0), Some(ptr_1)) = (ptr_0, ptr_1) {
                        let ptr = allocator.new_pair(*ptr_0, *ptr_1).unwrap();
                        self.add(py, node, &ptr)?;
                        Ok(None)
                    } else {
                        Ok(Some((p0.to_object(py), p1.to_object(py))))
                    }
                }
                _ => py_raise(py, "py view is None"),
            }
        })?;

        let r = self.from_py_to_native_cache(py, obj);
        r
    }

    pub fn native_for_py(
        &self,
        py: Python,
        obj: &PyCell<PyNaNode>,
        allocator: &mut IntAllocator,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        self.from_py_to_native_cache(py, obj)
            .or_else(|err| self.populate_native(py, obj, allocator))
    }

    // native to py methods

    fn from_native_to_py_cache<'p>(
        &'p self,
        py: Python<'p>,
        ptr: &<IntAllocator as Allocator>::Ptr,
    ) -> PyResult<&'p PyCell<PyNaNode>> {
        let locals = [
            ("cache", self.native_to_py.clone()),
            ("key", ptr.to_object(py)),
        ]
        .into_py_dict(py);
        py.eval("cache[key]", None, Some(locals))?.extract()
    }

    fn populate_python<'p>(
        &'p self,
        py: Python<'p>,
        ptr: &<IntAllocator as Allocator>::Ptr,
        allocator: &mut IntAllocator,
    ) -> PyResult<&'p PyCell<PyNaNode>> {
        apply_to_tree(ptr.clone(), move |ptr| {
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
                        PyNaNode::new(py, Some(PyView::new_atom(py, py_bytes)), None)?,
                        &ptr,
                    )?;
                    Ok(None)
                }
                SExp::Pair(ptr_1, ptr_2) => {
                    // we can only create this if the children are in the cache
                    // Let's fine out
                    let locals = [
                        ("cache", self.native_to_py.clone()),
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
                            let (p1, p2): (&PyCell<PyNaNode>, &PyCell<PyNaNode>) =
                                tuple.extract()?;
                            self.add(
                                py,
                                PyNaNode::new(
                                    py,
                                    Some(PyView::new_pair(py, PyTuple::new(py, &[p1, p2]))?),
                                    None,
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
        &'p self,
        py: Python<'p>,
        ptr: &<IntAllocator as Allocator>::Ptr,
        allocator: &mut IntAllocator,
    ) -> PyResult<&'p PyCell<PyNaNode>> {
        self.from_native_to_py_cache(py, ptr)
            .or_else(|err| Ok(self.populate_python(py, ptr, allocator)?))
    }
}

pub fn apply_to_tree<T, F>(mut node: T, mut apply: F) -> PyResult<()>
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

fn py_raise<T>(py: Python, msg: &str) -> PyResult<T> {
    let locals = [("msg", msg.to_object(py))].into_py_dict(py);

    py.run("raise RuntimeError(msg)", None, Some(locals))?;
    panic!("we should never get here")
}
