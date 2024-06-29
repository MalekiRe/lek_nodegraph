use std::collections::{HashMap, HashSet};
use bevy::reflect::{Reflect, ReflectMut, ReflectRef};

#[derive(Debug, Default)]
pub struct IndirectStack<'a> {
    values: Vec<StackValue<'a>>,
    field_mapping: HashMap<usize, (String, usize)>,
}

#[derive(Debug)]
pub enum StackValue<'a> {
    Owned(Box<dyn Reflect>),
    Mut(&'a mut dyn Reflect),
    InternalReference {
        name: String,
        parent: usize,
    }
}

impl<'a> IndirectStack<'a> {

    pub fn push(&mut self, stack_value: StackValue<'a>) {
        self.values.push(stack_value)
    }

    pub fn push_owned(&mut self, owned: Box<dyn Reflect>) {
        self.values.push(StackValue::Owned(owned))
    }
    pub fn push_internal_ref(&mut self, name: String, parent: usize) {
        self.values.push(StackValue::InternalReference {
            name,
            parent,
        });
    }
    pub fn push_mut(&mut self, r#mut: &'a mut dyn Reflect) {
        self.values.push(StackValue::Mut(r#mut))
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn pop(&mut self) -> Option<StackValue<'a>> {
        self.values.pop()
    }

    pub unsafe fn get_internal_from_ref(&self, parent: usize, name: String) -> Option<&'a dyn Reflect> {
        let mut thing = self.get_ref_internal(parent)?;
        let thing = match thing.reflect_ref() {
            ReflectRef::Struct(dyn_struct) => {
                dyn_struct.field(&name)?
            }
            _ => todo!(),
        };
        let thing = thing as *const dyn Reflect;
        Some(unsafe { &*thing})
    }

    pub unsafe fn get_mut_internal_from_ref(&mut self, parent: usize, name: String) -> Option<&'a mut dyn Reflect>{
        let mut thing = self.get_mut_internal(parent)?;
        let thing = match thing.reflect_mut() {
            ReflectMut::Struct(dyn_struct) => {
                dyn_struct.field_mut(&name)?
            }
            _ => todo!(),
        };
        let thing = thing as *mut dyn Reflect;
        Some(unsafe { &mut *thing})
    }

    pub unsafe fn get_ref_internal(&self, index: usize) -> Option<&'a dyn Reflect> {
        let is_real = match self.values.get(index)? {
            StackValue::Owned(_) => true,
            StackValue::Mut(_) => true,
            StackValue::InternalReference { .. } => false,
        };

        if is_real {
            let thing = match self.values.get(index)? {
                StackValue::Owned(ref dyn_reflect) => { dyn_reflect.as_ref() },
                StackValue::Mut(dyn_reflect) => {dyn_reflect.as_reflect()}
                StackValue::InternalReference { .. } => unreachable!(),
            } as *const dyn Reflect;

            let thing = unsafe { &*thing};
            Some(thing)
        } else {
            let (name, parent) = match self.values.get(index).unwrap() {
                StackValue::InternalReference { name, parent} => (name.clone(), *parent),
                _ => unreachable!(),
            };
            self.get_internal_from_ref(parent, name)
        }
    }

    pub unsafe fn get_mut_internal(&mut self, index: usize) -> Option<&'a mut dyn Reflect> {
        let is_real = match self.values.get(index)? {
            StackValue::Owned(_) => true,
            StackValue::Mut(_) => true,
            StackValue::InternalReference { .. } => false,
        };

        if is_real {
            let thing = match self.values.get_mut(index)? {
                StackValue::Owned(ref mut dyn_reflect) => { dyn_reflect.as_mut() },
                StackValue::Mut(dyn_reflect) => {dyn_reflect.as_reflect_mut()}
                StackValue::InternalReference { .. } => unreachable!(),
            } as *mut dyn Reflect;

            let thing = unsafe { &mut *thing};
            Some(thing)
        } else {
            let (name, parent) = match self.values.get(index).unwrap() {
                StackValue::InternalReference { name, parent} => (name.clone(), *parent),
                _ => unreachable!(),
            };
            self.get_mut_internal_from_ref(parent, name)
        }
    }
}