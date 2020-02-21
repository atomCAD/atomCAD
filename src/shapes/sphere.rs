use crate::jit::Jit;
use crate::implicit::{Implicit, Point, Units, Parameter, ParameterSettings, Description, Id};

pub struct Sphere {
    id: Id,
    pub pos: Point,
    pub radius: f64,
}

impl<JIT: Jit> Implicit<JIT> for Sphere {
    fn id(&self) -> Id {
        self.id
    }
    
    fn describe(&self) -> Description {
        Description {
            name: "Sphere".into(),
            description: "A sphere with radius `radius` positioned at `center`.".into(),
            parameters: vec![
                Parameter {
                    name: "radius".into(),
                    description: "The radius of the sphere.".into(),
                    settings: ParameterSettings::Scaler {
                        bounds: (Some(0.0), None),
                    },
                    default_units: Units::Nanometer,
                },
                Parameter {
                    name: "position".into(),
                    description: "The location where the sphere is centered.".into(),
                    settings: ParameterSettings::Vec3,
                    default_units: Units::Nanometer,
                },
            ],
        }
    }
    
    fn compile(&self, mut jit: JIT, xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let radius = jit.parameter("radius")?;
        let center = jit.parameter("position")?;

        let distance_to_center = jit.length2(center, xyz)?;
        let distance_to_surface = jit.subtract(distance_to_center, radius)?;

        jit.end(distance_to_surface)
    }
}
