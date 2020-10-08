use render::{AtomKind, AtomRepr, Fragment, GlobalRenderResources, Part, World};
use periodic_table::Element;
use ultraviolet::Vec3;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_till, take_while},
    combinator::map,
    multi::{fold_many0, many0, separated_list},
    number::complete::float,
    sequence::{delimited, terminated, tuple},
    IResult,
};
use std::collections::HashMap;
use std::path::Path;
use std::fs;

fn position(input: &str) -> IResult<&str, AtomRepr> {
    map(
        delimited(
            tag("["),
            tuple((
                terminated(float, tuple((tag(","), take_while(char::is_whitespace)))),
                terminated(float, tuple((tag(","), take_while(char::is_whitespace)))),
                float,
            )),
            tuple((take_while(char::is_whitespace), tag("]"))),
        ),
        |(x, y, z)| AtomRepr {
            pos: Vec3::new(x * 10.0, y * 10.0, z * 10.0),
            kind: AtomKind::new(Element::Carbon), // we don't have an element so just say carbon for now
        },
    )(input.trim_start_matches(|c: char| c.is_ascii_whitespace()))
}

fn array0(input: &str) -> IResult<&str, Vec<AtomRepr>> {
    delimited(tag("["), separated_list(tag(","), position), tag("]"))(
        input.trim_start_matches(|c: char| c.is_ascii_whitespace()),
    )
}

fn array1(input: &str) -> IResult<&str, Vec<AtomRepr>> {
    terminated(
        delimited(
            tag("["),
            fold_many0(terminated(array0, tag(",")), Vec::new(), |mut acc, new| {
                acc.extend(new.into_iter());
                acc
            }),
            tuple((take_while(|c: char| c.is_ascii_whitespace()), tag("];"))),
        ),
        take_while(|c: char| c == ','),
    )(input.trim_start_matches(|c: char| c.is_ascii_whitespace()))
}

fn variable(input: &str) -> IResult<&str, (String, Vec<AtomRepr>)> {
    let (input, _) = take_while(|c: char| c.is_ascii_whitespace())(input)?;
    let (input, _) = tag("var ")(input)?;
    let (input, var_name) = take_till(|c| c == '=')(input)?;
    let (input, _) = take_till(|c| c == '[')(input)?;
    // let (input, _) = tuple((take_till(|c| c == '='), tag("=")))(input)?;
    let (input, positions) = array1(input)?;

    Ok((input, (var_name.to_string(), positions)))
}

fn all_variables(input: &str) -> IResult<&str, HashMap<String, Vec<AtomRepr>>> {
    let (input, variables) = many0(variable)(input)?;

    Ok((input, variables.into_iter().collect()))
}

// Ignore this, this is related to some local DNA rendering work I was doing.
fn load_from_ti<P: AsRef<Path>>(
    render_resources: &GlobalRenderResources,
    path: P,
) -> Result<World, String> {
    let path = path.as_ref();

    if !path.exists() {
        return Err("path does not exist".to_string());
    }

    // just load it all into memory
    let text = fs::read_to_string(path).map_err(|_| "failed to load file")?;

    let (_, parts) = all_variables(&text).map_err(|_| "failed to parse")?;

    let mut world = World::new();

    for (name, atoms) in parts.iter() {
        println!("name: {}", name);
        if !name.starts_with("T") {
            let fragment = Fragment::from_atoms(render_resources, atoms.iter().copied());
            let part = Part::from_fragments(&mut world, name, Some(fragment));
            world.spawn_part(part);
        }
        
    }

    Ok(world)
}

pub fn load_ti_and_adjust(render_resources: &GlobalRenderResources) -> World {
    let mut ti_parts = load_from_ti(&render_resources, std::env::args().nth(1).unwrap()).unwrap();
    println!("Loaded {} parts and {} fragments", ti_parts.parts().len(), ti_parts.fragments().len());

    let first_corner = ti_parts.find_part("Cube").unwrap();
    let second_corner = ti_parts.copy_part(&render_resources, first_corner);
    let third_corner = ti_parts.copy_part(&render_resources, first_corner);

    {
        let corner = ti_parts.part_mut(first_corner);
        corner.rotate_by(0.0, -54.7356, 0.0);
        corner.offset_by(0.0, 45.9619407771 * 10.0, (65.0/2.0) * 10.0);
    }

    {
        let corner = ti_parts.part_mut(second_corner);
        corner.rotate_by(
            -60.000,
            54.7356,
            0.0
        );
        corner.offset_by(0.0, -45.9619407771 * 10.0, (65.0/2.0) * 10.0);
    }

    {
        let corner = ti_parts.part_mut(third_corner);
        corner.rotate_by(
            -30.0,
            0.0,
            -125.0,
        );
        corner.offset_by(45.9619407771 * 10.0, 0.0, -(65.0/2.0) * 10.0);
    }

    let fourth_corner = ti_parts.copy_part(&render_resources, third_corner);

    {
        let corner = ti_parts.part_mut(fourth_corner);
        corner.rotate_by(
            180.0,
            0.0,
            0.0,
        );
        corner.offset_by(-2.0 * 45.9619407771 * 10.0, 0.0, 0.0);
    }

    ti_parts
}
