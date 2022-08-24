pub struct Scene {
    pub palette: usize,
    pub code: usize,
    pub video1: usize,
    pub video2: usize,
}

// Static data for the game. Defines scenes
// and which data should be loaded for each
pub const SCENES: [Scene; 9] = [
    // Copy protection (0)
    Scene {
        palette: 0x14,
        code: 0x15,
        video1: 0x16,
        video2: 0x00,
    },
    // Intro (1)
    Scene {
        palette: 0x17,
        code: 0x18,
        video1: 0x19,
        video2: 0x00,
    },
    // Game begins (2)
    Scene {
        palette: 0x1a,
        code: 0x1b,
        video1: 0x1c,
        video2: 0x11,
    },
    // Jail (3)
    Scene {
        palette: 0x1d,
        code: 0x1e,
        video1: 0x1f,
        video2: 0x11,
    },
    Scene {
        palette: 0x20,
        code: 0x21,
        video1: 0x22,
        video2: 0x11,
    },
    // Tank (5)
    Scene {
        palette: 0x23,
        code: 0x24,
        video1: 0x25,
        video2: 0x00,
    },
    // Bath (6)
    Scene {
        palette: 0x26,
        code: 0x27,
        video1: 0x28,
        video2: 0x11,
    },
    // End sequence (7)
    Scene {
        palette: 0x29,
        code: 0x2a,
        video1: 0x2b,
        video2: 0x11,
    },
    // Password (8)
    Scene {
        palette: 0x7d,
        code: 0x7e,
        video1: 0x7f,
        video2: 0x00,
    },
];
