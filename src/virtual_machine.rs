use std::any::Any;
use std::collections::HashMap;
use std::ptr::NonNull;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::FilteredEntityMut;
use bevy::prelude::{AppTypeRegistry, Mut, QueryBuilder, Reflect, Res, Vec3, World};
use bevy::ptr::PtrMut;
use bevy::reflect::func::{Arg, ArgList, Return};
use bevy::reflect::{ReflectFromPtr, ReflectMut, ReflectRef, TypeInfo};
use crate::{functions};
use crate::indirect_stack::{IndirectStack, StackValue};

#[derive(Debug)]
pub enum Bytecode<'a> {
    Push(StackValue<'a>),
    Pop,
    Call(String),
    GetField(usize, String),
    SetField(usize),
    Query {
        components: Vec<(String, ComponentId, TypeInfo)>,
    },
    Copy(usize)
}

impl Clone for Bytecode<'_> {
    fn clone(&self) -> Self {
        match self {
            Bytecode::Push(val) => {
                match val {
                    StackValue::Owned(owned) => {
                        Bytecode::Push(StackValue::Owned(owned.clone_value()))
                    }
                    _ => panic!("can't clone if it's not owned"),
                }
            }
            Bytecode::Pop => Bytecode::Pop,
            Bytecode::Call(name) => Bytecode::Call(name.clone()),
            Bytecode::GetField(i, name) => Bytecode::GetField(*i, name.clone()),
            Bytecode::SetField(i) => Bytecode::SetField(*i),
            Bytecode::Query { components } => Bytecode::Query { components: components.clone() },
            Bytecode::Copy(i) => Bytecode::Copy(*i),
        }
    }
}


pub fn run(mut instructions: Vec<Bytecode>, world: &mut World) {

    //println!("{:#?}", instructions);

    world.resource_scope(|world, registry: Mut<AppTypeRegistry>| {
        let registry = registry.read();
        let mut functions = functions();

        // first instruction is a query
        match instructions.remove(0) {
            Bytecode::Query { components } => {
                let mut builder = QueryBuilder::<FilteredEntityMut>::new(world);
                for (name, id, type_info) in &components {
                    builder.mut_id(*id);
                }
                let mut query = builder.build();
                for mut filtered_entity in query.iter_mut(world) {
                    let instructions = instructions.clone();
                    let mut indirect_stack = IndirectStack::default();

                    let ids = filtered_entity.components().map(|a| a).collect::<Vec<_>>();

                    for id in ids {
                        let temp = filtered_entity.get_mut_by_id(id);
                        let mut temp2 = temp.unwrap();
                        let ptr = NonNull::new(temp2.as_mut().as_ptr()).unwrap();
                        let ptr = unsafe { std::mem::transmute(ptr)};
                        for (_, id2, type_info) in &components {
                            if *id2 == id {
                                let reflect_data = registry.get(type_info.type_id()).unwrap();
                                let reflect_from_ptr = reflect_data.data::<ReflectFromPtr>().unwrap();
                                let value = unsafe { reflect_from_ptr.as_reflect_mut(ptr) };
                                indirect_stack.push_mut(value);
                                break;
                            }
                        }
                    }
                    for instruction in instructions {
                        match instruction {
                            Bytecode::Push(val) => {
                                indirect_stack.push(val);
                            }
                            Bytecode::Pop => {
                                indirect_stack.pop();
                            }
                            Bytecode::Call(function) => {
                                let (func, arg_number) = functions.get_mut(&function).unwrap();
                                let mut args = ArgList::new();
                                for _ in 0..*arg_number {
                                    args = args.push(match indirect_stack.pop().unwrap() {
                                        StackValue::Owned(awa) => {
                                            Arg::Owned(awa)
                                        }
                                        StackValue::Mut(uwu) => {
                                            Arg::Mut(uwu)
                                        }
                                        StackValue::InternalReference { name, parent } => {
                                            unsafe { Arg::Mut(indirect_stack.get_mut_internal_from_ref(parent, name).unwrap()) }
                                        }
                                    });
                                }
                                match func.call(args).unwrap() {
                                    Return::Unit => {}
                                    Return::Owned(owned) => indirect_stack.push_owned(owned),
                                    Return::Ref(r#ref) => todo!(),
                                    Return::Mut(r#mut) => indirect_stack.push_mut(r#mut),
                                };
                            }
                            Bytecode::GetField(index, field_name) => {
                                indirect_stack.push_internal_ref(field_name, index);
                            },
                            Bytecode::SetField(index) => {
                                let first= indirect_stack.pop().unwrap();
                                match first {
                                    StackValue::Owned(owned) => {
                                        unsafe { indirect_stack.get_mut_internal(index).unwrap().apply(owned.as_ref()) };
                                    }
                                    StackValue::Mut(dyn_reflect) => {
                                        unsafe {
                                            indirect_stack.get_mut_internal(index).unwrap().apply(dyn_reflect.as_reflect());
                                        }
                                    },
                                    StackValue::InternalReference { name, parent } => {
                                        unsafe {
                                            let first = indirect_stack.get_internal_from_ref(parent, name).unwrap();
                                            indirect_stack.get_mut_internal(index).unwrap().apply(first);
                                        }
                                    },
                                }
                            },
                            Bytecode::Query { .. } => panic!("shouldn't have a second query"),
                            Bytecode::Copy(index) => {
                                let val = unsafe { indirect_stack.get_ref_internal(index).unwrap() }.clone_value();
                                indirect_stack.push_owned(val);
                            },
                        }
                    }
                }
            }
            awa => panic!("first instruction should be a query, instead: {:#?}", awa),
        }
    });
}
