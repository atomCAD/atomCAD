use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::renderer::line_mesh::LineMesh;
use glam::f32::Vec3;

/// Color for unit cell wireframe edges.
const UNIT_CELL_WIREFRAME_COLOR: [f32; 3] = [0.75, 0.75, 0.75]; // light gray

/// Tessellates a unit cell as 12 wireframe edges (parallelepiped) into a LineMesh.
///
/// The 8 vertices of the parallelepiped are formed from origin (0,0,0) and the
/// three basis vectors a, b, c:
///   O, O+a, O+b, O+c, O+a+b, O+a+c, O+b+c, O+a+b+c
///
/// The 12 edges connect these vertices along each basis direction.
pub fn tessellate_unit_cell_wireframe(line_mesh: &mut LineMesh, unit_cell: &UnitCellStruct) {
    tessellate_unit_cell_wireframe_with_color(line_mesh, unit_cell, &UNIT_CELL_WIREFRAME_COLOR);
}

/// Tessellates a unit cell wireframe with a custom color.
pub fn tessellate_unit_cell_wireframe_with_color(
    line_mesh: &mut LineMesh,
    unit_cell: &UnitCellStruct,
    color: &[f32; 3],
) {
    let a = unit_cell.a;
    let b = unit_cell.b;
    let c = unit_cell.c;

    // 8 vertices of the parallelepiped
    let o = Vec3::ZERO;
    let va = Vec3::new(a.x as f32, a.y as f32, a.z as f32);
    let vb = Vec3::new(b.x as f32, b.y as f32, b.z as f32);
    let vc = Vec3::new(c.x as f32, c.y as f32, c.z as f32);
    let vab = va + vb;
    let vac = va + vc;
    let vbc = vb + vc;
    let vabc = va + vb + vc;

    // 12 edges: 4 along each basis direction
    // Edges along a-direction
    line_mesh.add_line_with_uniform_color(&o, &va, color);
    line_mesh.add_line_with_uniform_color(&vb, &vab, color);
    line_mesh.add_line_with_uniform_color(&vc, &vac, color);
    line_mesh.add_line_with_uniform_color(&vbc, &vabc, color);

    // Edges along b-direction
    line_mesh.add_line_with_uniform_color(&o, &vb, color);
    line_mesh.add_line_with_uniform_color(&va, &vab, color);
    line_mesh.add_line_with_uniform_color(&vc, &vbc, color);
    line_mesh.add_line_with_uniform_color(&vac, &vabc, color);

    // Edges along c-direction
    line_mesh.add_line_with_uniform_color(&o, &vc, color);
    line_mesh.add_line_with_uniform_color(&va, &vac, color);
    line_mesh.add_line_with_uniform_color(&vb, &vbc, color);
    line_mesh.add_line_with_uniform_color(&vab, &vabc, color);
}
