use bevy::app::{App, Plugin};
use bevy::ecs::component::ComponentId;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::reflect::func::{Function, IntoFunction};
use std::any::TypeId;
use std::collections::HashMap;

pub trait RegisterFunction<T> {
    fn register_function(&mut self, name: impl AsRef<str>, function: impl IntoFunction<'static, T>);
}

impl<T> RegisterFunction<T> for App {
    fn register_function(
        &mut self,
        name: impl AsRef<str>,
        function: impl IntoFunction<'static, T>,
    ) {
        let input = (name.as_ref().to_string(), function.into_function());
        self.world_mut().run_system_once_with(
            (input),
            |thing: In<(String, Function<'static>)>, mut res: NonSendMut<FunctionRegistry>| -> () {
                res.0.insert(thing.0 .0, thing.0 .1);
            },
        );
    }
}

pub struct RegistryPlugin;

impl Plugin for RegistryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send_resource(FunctionRegistry::default());
        app.add_systems(PostStartup, |world: &mut World| {
            world.resource_scope(|world: &mut World, registry: Mut<AppTypeRegistry>| {
                let mut map = HashMap::new();
                for a in registry.read().iter() {
                    if let Some(component_id) = world.components().get_id(a.type_id()) {
                        map.insert(a.type_id(), component_id);
                    }
                }
                world.insert_resource(ComponentMap(map));
            });
        });
    }
}

#[derive(Resource)]
pub struct ComponentMap(pub HashMap<TypeId, ComponentId>);

#[derive(Default)]
pub struct FunctionRegistry(pub HashMap<String, Function<'static>>);
