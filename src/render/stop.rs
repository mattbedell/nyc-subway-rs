use std::cmp::Ordering;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
// this is pretty much a Vertex currently, an Instance struct may not be needed
pub struct StopInstance {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub scale: f32,
}

impl StopInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![4 => Float32x3, 5 => Float32x3, 6 => Float32];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<StopInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

impl From<StopState> for StopInstance {
    fn from(value: StopState) -> Self {
        match value {
            StopState::Active(a) => a,
            StopState::Inactive(a) => a,
        }
    }
}

impl Default for StopInstance {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0],
            scale: 0.0,
        }
    }
}

#[derive(Debug)]
pub enum StopState {
    Inactive(StopInstance),
    Active(StopInstance),
}

impl Ord for StopState {
    fn cmp(&self, other: &Self) -> Ordering {
        match self {
            Self::Inactive(_) => match other {
                Self::Inactive(_) => Ordering::Equal,
                Self::Active(_) => Ordering::Less,
            },
            Self::Active(_) => match other {
                Self::Active(_) => Ordering::Equal,
                Self::Inactive(_) => Ordering::Greater,
            },
        }
    }
}

impl PartialOrd for StopState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for StopState {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for StopState {}
