use std::cell::{RefCell, RefMut};
use std::collections::HashSet;

use pyo3::prelude::pyclass;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyBytes, PyTuple};

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;
use crate::serialize::node_from_bytes;

use super::py_view::PyView;

#[pyclass(subclass, unsendable)]
pub struct PyArena {
    arena: RefCell<IntAllocator>,
    cache: PyObject,
    to_python: PyObject,
}

#[pymethods]
impl PyArena {
    #[new]
    pub fn new(py: Python, new_obj_f: PyObject) -> PyResult<Self> {
        Ok(PyArena {
            arena: RefCell::new(IntAllocator::default()),
            cache: py.eval("dict()", None, None)?.to_object(py),
            to_python: new_obj_f,
        })
    }

    pub fn deserialize<'p>(&self, py: Python<'p>, blob: &[u8]) -> PyResult<&'p PyAny> {
        let allocator: &mut IntAllocator = &mut self.allocator() as &mut IntAllocator;
        let ptr = node_from_bytes(allocator, blob)?;
        self.py_for_native(py, &ptr, allocator)
    }

    pub fn include<'p>(&self, py: Python<'p>, obj: &'p PyAny) -> PyResult<&'p PyAny> {
        let ptr = Self::ptr_for_obj(self, py, obj)?;
        self.py_for_native(py, &ptr, &mut self.allocator() as &mut IntAllocator)
    }

    pub fn ptr_for_obj(&self, py: Python, obj: &PyAny) -> PyResult<i32> {
        let allocator: &mut IntAllocator = &mut self.allocator() as &mut IntAllocator;
        self.populate_native(py, obj, allocator)
    }
}

impl PyArena {
    pub fn new_cell_obj(py: Python, new_obj_f: PyObject) -> PyResult<&PyCell<Self>> {
        PyCell::new(py, PyArena::new(py, new_obj_f)?)
    }

    pub fn new_cell(py: Python) -> PyResult<&PyCell<Self>> {
        Self::new_cell_obj(py, py.eval("lambda sexp: sexp", None, None)?.to_object(py))
    }

    pub fn obj_for_ptr<'p>(&self, py: Python<'p>, ptr: i32) -> PyResult<&'p PyAny> {
        self.py_for_native(py, &ptr, &mut self.allocator())
    }

    pub fn allocator(&self) -> RefMut<IntAllocator> {
        self.arena.borrow_mut()
    }

    pub fn add(
        &self,
        py: Python,
        obj: &PyAny,
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
        obj: &PyAny,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        let locals = [("cache", self.cache.clone()), ("key", obj.to_object(py))].into_py_dict(py);
        py.eval("cache.get(id(key))", None, Some(locals))?.extract()
    }

    fn populate_native(
        &self,
        py: Python,
        obj: &PyAny,
        allocator: &mut IntAllocator,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        // items in `pending` are already in the stack of things to be converted
        // if they appear again, we have an illegal cycle and must fail

        let mut pending: HashSet<usize> = HashSet::new();

        apply_to_tree(obj, move |obj| {
            // is it in cache yet?
            if self.from_py_to_native_cache(py, obj).is_ok() {
                // yep, we're done
                return Ok(None);
            }

            // it's not in the cache

            match PyView::py_view_for_obj(obj)? {
                PyView::Atom(atom) => {
                    let blob: &[u8] = atom.extract(py).unwrap();
                    let ptr = allocator.new_atom(blob).unwrap();
                    self.add(py, obj, &ptr)?;

                    Ok(None)
                }
                PyView::Pair(pair) => {
                    let pair: &PyAny = pair.into_ref(py);
                    let pair: &PyTuple = pair.extract()?;
                    let p0: &PyAny = pair.get_item(0);
                    let p1: &PyAny = pair.get_item(1);
                    let ptr_0: PyResult<i32> = self.from_py_to_native_cache(py, p0);
                    let ptr_1: PyResult<i32> = self.from_py_to_native_cache(py, p1);

                    let as_obj = id_for_pyany(py, obj)?;

                    if let (Ok(ptr_0), Ok(ptr_1)) = (ptr_0, ptr_1) {
                        let ptr = allocator.new_pair(ptr_0, ptr_1).unwrap();
                        self.add(py, obj, &ptr)?;

                        pending.remove(&as_obj);
                        Ok(None)
                    } else {
                        if pending.contains(&as_obj) {
                            let locals = Some([("obj", obj)].into_py_dict(py));
                            py.run(
                                "raise ValueError(f'illegal clvm object loop {obj}')",
                                None,
                                locals,
                            )?;
                            panic!();
                        }
                        pending.insert(as_obj);

                        Ok(Some((p0, p1)))
                    }
                }
            }
        })?;

        self.from_py_to_native_cache(py, obj)
    }

    pub fn native_for_py(
        slf: &PyCell<PyArena>,
        py: Python,
        obj: &PyAny,
        allocator: &mut IntAllocator,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        let celf = slf.borrow();
        celf.from_py_to_native_cache(py, obj)
            .or_else(|_err| celf.populate_native(py, obj, allocator))
    }

    // native to py methods

    fn from_native_to_py_cache<'p>(
        &self,
        py: Python<'p>,
        ptr: &<IntAllocator as Allocator>::Ptr,
    ) -> PyResult<&'p PyAny> {
        let locals = [("cache", self.cache.clone()), ("key", ptr.to_object(py))].into_py_dict(py);
        py.eval("cache[key]", None, Some(locals))?.extract()
    }

    fn populate_python<'p>(
        &self,
        py: Python<'p>,
        ptr: &<IntAllocator as Allocator>::Ptr,
        allocator: &mut IntAllocator,
    ) -> PyResult<&'p PyAny> {
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
                    self.add(py, self.to_python.as_ref(py).call1((py_bytes,))?, &ptr)?;
                    Ok(None)
                }
                SExp::Pair(ptr_1, ptr_2) => {
                    // we can only create this if the children are in the cache
                    // Let's find out
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
                            let (_p1, _p2): (&PyAny, &PyAny) = tuple.extract()?;
                            self.add(py, self.to_python.as_ref(py).call1((tuple,))?, &ptr)?;
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
    ) -> PyResult<&'p PyAny> {
        self.from_native_to_py_cache(py, ptr)
            .or_else(|_err| self.populate_python(py, ptr, allocator))
    }
}

fn id_for_pyany(py: Python, obj: &PyAny) -> PyResult<usize> {
    let locals = Some([("obj", obj)].into_py_dict(py));
    py.eval("id(obj)", None, locals)?.extract()
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
