use std::{ops::{Deref, DerefMut}, hash::{Hash, BuildHasher, Hasher}};

use bevy::{prelude::*, utils::{HashMap, StableHashSet}, math::const_vec2, core::FixedTimestep};

fn main() {
  App::new()
    .add_plugins(DefaultPlugins)
    .insert_resource(ParticleLookup::new(40, 20))
    .add_event::<ParticleCollisionEvent>()
    .add_startup_system(setup)
    .add_system(handle_collisions.label("collisions"))
    .add_system_set(SystemSet::new()
      .with_run_criteria(FixedTimestep::step(0.25))
      .with_system(discover_collisions.label("discover").after("collisions"))
      .with_system(handle_movement.after("discover"))
    )
    .run();
}

pub trait BoundsExt {
  fn outside(&self, point: Vec2) -> Option<Vec2>;
  fn min(&self) -> Vec2;
  fn max(&self) -> Vec2;
}

impl BoundsExt for Rect<f32> {
  fn outside(&self, point: Vec2) -> Option<Vec2> {
    let mut normal = Vec2::ZERO;
    if point.x < self.left {
      normal.x = 1.;
    } else if point.x > self.right {
      normal.x = -1.;
    }
    if point.y < self.bottom {
      normal.y = 1.;
    } else if point.y > self.top {
      normal.y = -1.;
    }
    if normal != Vec2::ZERO {
      Some(normal)
    } else {
      None
    }
  }

  fn min(&self) -> Vec2 {
    Vec2::new(self.left, self.bottom)
  }

  fn max(&self) -> Vec2 {
    Vec2::new(self.right, self.top)
  }
}

#[derive(Clone)]
pub struct ParticleLookup {
  bounds: Rect<f32>,
  particles: HashMap<IVec2, Entity>,
}

impl ParticleLookup {
  pub fn new(width: i32, height: i32) -> Self {
    Self {
      bounds: Rect::<f32> {
        left: -width as f32 / 2.,
        right: width as f32 / 2.,
        top: height as f32 / 2.,
        bottom: -height as f32 / 2.,
      },
      particles: HashMap::new(),
    }
  }
}

impl Deref for ParticleLookup {
  type Target = HashMap<IVec2, Entity>;

  fn deref(&self) -> &Self::Target {
    &self.particles
  }
}

impl DerefMut for ParticleLookup {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.particles
  }
}

#[derive(Component)]
pub struct Particle {
  pub position: Vec2,
  pub velocity: Vec2,
  pub mass: f32,
  pub elasticity: f32,
}

impl Particle {
  const SPRITE_SIZE: f32 = 16.0;
  const GRAVITY: Vec2 = const_vec2!([0., -1.]);

  pub fn new(position: Vec2, mass: f32) -> Self {
    Self { position, velocity: Vec2::ZERO, mass, elasticity: 0.5 }
  }
}

pub enum ParticleCollisionEvent {
  World(Entity, Vec2),
  Particle(Entity, Entity),
}

fn setup(mut commands: Commands) {
  commands.spawn_bundle(OrthographicCameraBundle::new_2d());

  for x in -0..3 {
    if x == 0 { continue }
    commands
      .spawn_bundle(SpriteBundle {
        transform: Transform::from_xyz(x as f32 * Particle::SPRITE_SIZE, 0.0, 0.0),
        sprite: Sprite {
          color: if x < 0 { Color::WHITE } else if x == 1 { Color::BLUE } else { Color::RED },
          custom_size: Some(Vec2::splat(Particle::SPRITE_SIZE)),
          ..Default::default()
        },
        ..Default::default()
      })
      .insert(Particle {
        position: Vec2::new(x as f32, 0.),
        velocity: Vec2::new((x as f32).signum(), 0.),
        mass: 1.,
        elasticity: 0.4,
      });
  }
}

fn discover_collisions(
  particle_lookup: ResMut<ParticleLookup>,
  mut query: Query<(Entity, &mut Particle)>,
  mut collision_events: EventWriter<ParticleCollisionEvent>,
  time: Res<Time>,
) {
  let mut handled = StableHashSet::<u64>::default();
  for (entity, mut particle) in query.iter_mut() {
    particle.velocity += Particle::GRAVITY * time.delta_seconds();

    if particle.velocity != Vec2::ZERO {
      let current_point = particle.position.floor().as_ivec2();
      let potential_position = particle.position + particle.velocity;
      let potential_point = potential_position.floor().as_ivec2();

      if potential_point != current_point {
        if let Some(collision) = check_for_collision(entity, potential_position, &particle_lookup) {
          if let ParticleCollisionEvent::Particle(a, b) = collision {
            let mut hasher = handled.hasher().build_hasher();
            a.hash(&mut hasher);
            b.hash(&mut hasher);
            let hash = hasher.finish();
            if handled.contains(&hash) { continue; }

            let mut hasher = handled.hasher().build_hasher();
            b.hash(&mut hasher);
            a.hash(&mut hasher);
            let alternate = hasher.finish();
            if handled.contains(&alternate) { continue; }

            handled.insert(hash);
          }

          collision_events.send(collision);
        }
      }
    }
  }
}

fn calculate_collision(current: &Particle, other: &Particle) -> Vec2 {
  (current.elasticity * other.mass * (other.velocity - current.velocity) + current.mass * current.velocity + other.mass * other.velocity) / (current.mass + other.mass)
}

fn check_for_collision(
  entity: Entity,
  potential_position: Vec2,
  particle_lookup: &ParticleLookup
) -> Option<ParticleCollisionEvent> {
  let potential_point = potential_position.floor().as_ivec2();
  // println!("Looking at point {:?}", potential_point);
  if let Some(wall_normal) = particle_lookup.bounds.outside(potential_position) {
    Some(ParticleCollisionEvent::World(entity, wall_normal))
  } else if let Some(colliding_entity) = particle_lookup.get(&potential_point) {
    if *colliding_entity != entity {
      Some(ParticleCollisionEvent::Particle(entity, *colliding_entity))
    } else {
      None
    }
  } else {
    None
  }
}

fn resolve_particle(
  entity: Entity,
  mut particles: &mut Query<&mut Particle>,
  particle_lookup: &ParticleLookup,
) {
  if let Some(particle) = particles.get(entity).ok() {
    if particle.velocity != Vec2::ZERO {
      let current_point = particle.position.floor().as_ivec2();
      let potential_position = particle.position + particle.velocity;
      let potential_point = potential_position.floor().as_ivec2();

      // println!("Testing recursive collision: {:?} @ {:?} going to {:?}", entity, particle.position, potential_position);
      if potential_point != current_point {
        if let Some(collision) = check_for_collision(entity, potential_position, &particle_lookup) {
          println!("Recursive collision occured: {:?} {:?}", entity, particle.velocity);
          handle_collision(&collision, &mut particles, particle_lookup);
        }
      }
    }
  }
}

fn handle_collision(
  collision: &ParticleCollisionEvent,
  mut particles: & mut Query<&mut Particle>,
  particle_lookup: &ParticleLookup,
) {
  match collision {
    ParticleCollisionEvent::Particle(entity_a, entity_b) => {
      // TODO: If other entity is asleep awaken after after collision
      if let Ok([mut particle_a, mut particle_b]) = particles.get_many_mut([*entity_a, *entity_b]) {
        let new_a_velocity = calculate_collision(&particle_a, &particle_b);
        let new_b_velocity = calculate_collision(&particle_b, &particle_a);
        particle_a.velocity = (new_a_velocity * 100.).round() / 100.;
        particle_b.velocity = (new_b_velocity * 100.).round() / 100.;

        // println!("Particle collision occured: {:?} {:?} | {:?} {:?}", entity_a, particle_a.velocity, entity_b, particle_b.velocity);

        // We now need to check if applied velocity on b causes another collision
        resolve_particle(*entity_b, &mut particles, particle_lookup);

        // Now we need to check the new velocity to see if it will overlap on the
      }
      if let Ok([mut particle_a, particle_b]) = particles.get_many_mut([*entity_a, *entity_b]) {
        let new_a_velocity = calculate_collision(&particle_a, &particle_b);
        particle_a.velocity = (new_a_velocity * 100.).round() / 100.;

        // Not sure if we need to check this side recursively.
      }
    },
    ParticleCollisionEvent::World(entity, normal) => {
      if let Some(mut particle) = particles.get_mut(*entity).ok() {
        particle.velocity = particle.velocity - (1. + particle.elasticity) * (particle.velocity * (*normal)) * (*normal).normalize();
        particle.velocity = (particle.velocity * 100.).round() / 100.;

        // println!("Wall collision on {:?} {:?}", entity, particle.velocity);
        resolve_particle(*entity, particles, particle_lookup);
      }
    }
  }
}

fn handle_collisions(
  mut collision_events: EventReader<ParticleCollisionEvent>,
  mut particles: Query<&mut Particle>,
  particle_lookup: Res<ParticleLookup>,
) {
  for collision in collision_events.iter() {
    handle_collision(collision, &mut particles, &particle_lookup);
  }
}

fn handle_movement(
  mut query: Query<(Entity, &mut Particle, &mut Transform)>,
  mut particle_lookup: ResMut<ParticleLookup>,
) {
  for (entity, mut particle, mut transform) in query.iter_mut() {
    let current_point = particle.position.floor().as_ivec2();
    let new_position = particle.position + particle.velocity;
    let new_point = new_position.floor().as_ivec2();

    // println!("{:?} @ {:?} ({:?}) with {:?} going to {:?} ({:?})", entity, particle.position, current_point, particle.velocity, new_position, new_point);
    if current_point != new_point {
      if particle_lookup.get(&current_point) == Some(&entity) {
        particle_lookup.remove(&current_point);
      }
      particle_lookup.insert(new_point, entity);
    }
    particle.position = new_position;
    transform.translation = new_point.as_vec2().extend(0.) * Particle::SPRITE_SIZE;
  }
  // println!("----");
}
