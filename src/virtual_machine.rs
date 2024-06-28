use std::any::Any;
use std::ptr::NonNull;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::FilteredEntityMut;
use bevy::prelude::{AppTypeRegistry, Mut, QueryBuilder, Reflect, Res, World};
use bevy::ptr::PtrMut;
use bevy::reflect::func::{Arg, ArgList, Return};
use bevy::reflect::{ReflectFromPtr, TypeInfo};
use crate::{functions};

#[derive(Debug)]
pub enum Bytecode<'a> {
    Push(Arg<'a>),
    Pop,
    Call(String),
    GetField(usize),
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
                    Arg::Owned(owned) => {
                        Bytecode::Push(Arg::Owned(owned.clone_value()))
                    }
                    _ => panic!("can't clone if it's not owned"),
                }
            }
            Bytecode::Pop => Bytecode::Pop,
            Bytecode::Call(name) => Bytecode::Call(name.clone()),
            Bytecode::GetField(i) => Bytecode::GetField(*i),
            Bytecode::SetField(i) => Bytecode::SetField(*i),
            Bytecode::Query { components } => Bytecode::Query { components: components.clone() },
            Bytecode::Copy(i) => Bytecode::Copy(*i),
        }
    }
}

pub fn run(mut instructions: Vec<Bytecode>, world: &mut World) {

    println!("{:#?}", instructions);

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
                    let mut stack: Vec<Arg> = Vec::new();

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
                                stack.push(Arg::Mut(value));
                                break;
                            }
                        }
                    }
                    for instruction in instructions {
                        match instruction {
                            Bytecode::Push(val) => {
                                stack.push(val);
                            }
                            Bytecode::Pop => {
                                stack.pop();
                            }
                            Bytecode::Call(function) => {
                                let (func, arg_number) = functions.get_mut(&function).unwrap();
                                let mut args = ArgList::new();
                                for _ in 0..*arg_number {
                                    args = args.push(stack.pop().unwrap());
                                }
                                match func.call(args).unwrap() {
                                    Return::Unit => {}
                                    Return::Owned(owned) => stack.push(Arg::Owned(owned)),
                                    Return::Ref(r#ref) => stack.push(Arg::Ref(r#ref)),
                                    Return::Mut(r#mut) => stack.push(Arg::Mut(r#mut)),
                                };
                            }
                            Bytecode::GetField { .. } => todo!(),
                            Bytecode::SetField(index) => {
                                let Arg::Owned(first) = stack.pop().unwrap() else { panic!(); };
                                match stack.get_mut(index).unwrap() {
                                    Arg::Mut(ref mut awa) => {
                                        awa.apply(first.as_ref());
                                    }
                                    _ => panic!(),
                                }
                                stack.push(Arg::Owned(first));
                            },
                            Bytecode::Query { .. } => panic!("shouldn't have a second query"),
                            Bytecode::Copy(index) => {
                                let val = match stack.get(index).unwrap() {
                                    Arg::Owned(owned) => {
                                        owned.clone_value()
                                    }
                                    Arg::Ref(_) => todo!(),
                                    Arg::Mut(_) => todo!(),
                                };
                                stack.push(Arg::Owned(val));
                            },
                        }
                    }
                }
            }
            awa => panic!("first instruction should be a query, instead: {:#?}", awa),
        }
    });
}
