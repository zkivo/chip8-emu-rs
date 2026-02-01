use rand;
use std::time::{Duration, Instant};
use std::{env, fs, path::Path, process, thread::sleep};

use sdl3::event::Event;
use sdl3::keyboard::Keycode;
use sdl3::pixels::Color;

// CHIP-8 framebuffer size
const FB_WIDTH: u32 = 64;
const FB_HEIGHT: u32 = 32;

// Minifb window size
const WINDOW_WIDTH: u32 = FB_WIDTH * 15;
const WINDOW_HEIGHT: u32 = FB_HEIGHT * 15;

const MEMORY_SIZE: usize = 4096;
const ROM_START: usize = 0x200;

const FONT_START: usize = 0x050;
const FONT_BYTES: usize = 16 * 5;
const FONT: [u8; FONT_BYTES] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

struct VM {
    v: [u8; 16],
    pc: u16,
    i: u16,

    memory: [u8; MEMORY_SIZE],
    stack: Vec<u16>,
    framebuffer: [u8; (FB_WIDTH * FB_HEIGHT) as usize],
    draw_flag: bool,
    keyboard: [bool; 16],

    delay_timer: u8,
    sound_timer: u8,
}

impl VM {
    fn new() -> Self {
        VM {
            v: [0; 16],
            pc: ROM_START as u16,
            i: 0,
            memory: [0; MEMORY_SIZE],
            stack: Vec::new(),
            framebuffer: [0; (FB_WIDTH * FB_HEIGHT) as usize],
            delay_timer: 0,
            sound_timer: 0,
            keyboard: [false; 16],
            draw_flag: false,
        }
    }

    fn step_timers(&mut self) {
        if self.delay_timer > 0 {
            self.delay_timer -= 1;
        }
        if self.sound_timer > 0 {
            self.sound_timer -= 1;
        }
    }

    fn step(&mut self) {
        let opcode: u16 =
            (self.memory[self.pc as usize] as u16) << 8 | self.memory[self.pc as usize + 1] as u16;
        let nnn = opcode & 0x0FFF;
        let nn = (opcode & 0x00FF) as usize;
        let n = (opcode & 0x000F) as usize;
        let x = ((opcode & 0x0F00) >> 8) as usize;
        let y = ((opcode & 0x00F0) >> 4) as usize;
        self.pc += 2;
        match opcode & 0xF000 {
            0x0000 => {
                match opcode & 0x00FF {
                    0x00E0 => {
                        // CLEAR SCREEN
                        for idx in 0..(FB_WIDTH * FB_HEIGHT) as usize {
                            self.framebuffer[idx] = 0;
                        }
                        self.draw_flag = true;
                    }

                    0x00EE => {
                        // RET
                        let addr = self.stack.pop();
                        self.pc = addr.expect("REASON");
                    }

                    _ => { /* SYS / ignored */ }
                }
            }

            0x1000 => {
                // JUMP nnn
                self.pc = nnn;
            }

            0x2000 => {
                // CALL nnn
                self.stack.push(self.pc);
                self.pc = nnn;
            }

            0x3000 => {
                // SE Vx, byte
                if self.v[x] == nn as u8 {
                    self.pc += 2;
                }
            }

            0x4000 => {
                // SNE Vx, byte
                if self.v[x] != nn as u8 {
                    self.pc += 2;
                }
            }

            0x5000 => {
                // SE Vx, Vy
                if self.v[x] == self.v[y] {
                    self.pc += 2;
                }
            }

            0x6000 => {
                // LOAD Vx, nn
                self.v[x] = nn as u8;
            }

            0x7000 => {
                // ADD Vx, nn
                self.v[x] = self.v[x].wrapping_add(nn as u8);
            }

            0x8000 => {
                // Vx, Vy
                match n as u8 {
                    0 => {
                        // LD Vx, Vy
                        self.v[x] = self.v[y];
                    }

                    1 => {
                        // OR Vx, Vy
                        self.v[x] |= self.v[y];
                    }

                    2 => {
                        // AND Vx, Vy
                        self.v[x] &= self.v[y];
                    }

                    3 => {
                        // XOR Vx, Vy
                        self.v[x] ^= self.v[y];
                    }

                    4 => {
                        // ADD Vx, Vy
                        let (sum, carry) = self.v[x].overflowing_add(self.v[y]);
                        self.v[x] = sum;
                        self.v[0xF] = if carry { 1 } else { 0 };
                    }

                    5 => {
                        // SUB Vx, Vy
                        let (diff, borrow) = self.v[x].overflowing_sub(self.v[y]);
                        self.v[x] = diff;
                        self.v[0xF] = if borrow { 0 } else { 1 };
                    }

                    6 => {
                        // SHR Vx {, Vy}
                        self.v[0xF] = self.v[x] & 0x01;
                        self.v[x] >>= 1;
                    }

                    7 => {
                        // SUBN Vx, Vy
                        let (diff, borrow) = self.v[y].overflowing_sub(self.v[x]);
                        self.v[x] = diff;
                        self.v[0xF] = if borrow { 0 } else { 1 };
                    }

                    0x0E => {
                        // SHL Vx {, Vy}
                        self.v[0xF] = (self.v[x] & 0x80) >> 7;
                        self.v[x] <<= 1;
                    }

                    _ => {
                        // Unknown opcode
                    }
                }
            }

            0x9000 => {
                // SNE Vx, Vy
                if self.v[x] != self.v[y] {
                    self.pc += 2;
                }
            }

            0xA000 => {
                // LOAD i, nnn
                self.i = nnn;
            }

            0xB000 => {
                // JUMP V0, nnn
                self.pc = nnn + self.v[0] as u16;
            }

            0xC000 => {
                // RND Vx, byte
                let rnd_byte: u8 = rand::random::<u8>();
                self.v[x] = rnd_byte & (nn as u8);
            }

            0xD000 => {
                // DRAW Vx, Vy, n
                let vx = self.v[x] as usize;
                let vy = self.v[y] as usize;
                self.v[0xF] = 0;
                for row in 0usize..n as usize {
                    let sprite_byte = self.memory[self.i as usize + row];
                    for col in 0usize..8 {
                        let fb_idx = (((vy + row) % FB_HEIGHT as usize) * FB_WIDTH as usize)
                            + (vx + col) % FB_WIDTH as usize;
                        let fb_byte: u8 = self.framebuffer[fb_idx];
                        let sprite_pixel: u8 = (0b1000_0000 >> col) & sprite_byte;
                        if sprite_pixel != 0 && fb_byte == 0x00 {
                            // Light up pixel
                            self.framebuffer[fb_idx] = 0xFF;
                        } else if sprite_pixel != 0 && fb_byte == 0xFF {
                            // Turn off pixel, and set VF because of collision
                            self.v[0xF] = 1;
                            self.framebuffer[fb_idx] = 0x00;
                        }
                    }
                }
                self.draw_flag = true;
            }

            0xE000 => {
                match nn as u8 {
                    0x9E => {
                        // SKP Vx
                        let key = self.v[x] as usize;
                        if self.keyboard[key] {
                            self.pc += 2;
                        }
                    }

                    0xA1 => {
                        // SKNP Vx
                        let key = self.v[x] as usize;
                        if !self.keyboard[key] {
                            self.pc += 2;
                        }
                    }

                    _ => {
                        // Unknown opcode
                    }
                }
            }

            0xF000 => {
                match nn as u8 {
                    0x07 => {
                        // Vx = get_delay()
                        self.v[x] = self.delay_timer;
                    }

                    0x0A => {
                        // Vx = get_key()
                        for key in 0..16 {
                            if self.keyboard[key] {
                                self.v[x] = key as u8;
                                return;
                            }
                        }
                        self.pc -= 2;
                    }

                    0x15 => {
                        // delay_timer(Vx)
                        self.delay_timer = self.v[x];
                    }

                    0x18 => {
                        // sound_timer(Vx)
                        self.sound_timer = self.v[x];
                    }

                    0x1E => {
                        // ADD I, Vx
                        self.i = self.i.wrapping_add(self.v[x] as u16);
                    }

                    0x29 => {
                        // I = sprite_addr[Vx]
                        let digit = self.v[x] as u16;
                        self.i = FONT_START as u16 + (digit * 5);
                    }

                    0x33 => {
                        // set_BCD(Vx) *(I+0) = BCD(3); *(I+1) = BCD(2); *(I+2) = BCD(1);
                        let vx = self.v[x];
                        self.memory[self.i as usize] = vx / 100;
                        self.memory[self.i as usize + 1] = (vx % 100) / 10;
                        self.memory[self.i as usize + 2] = vx % 10;
                    }

                    0x55 => {
                        // LD [I], V0..Vx
                        for idx in 0..=x {
                            self.memory[self.i as usize + idx] = self.v[idx];
                        }
                    }

                    0x65 => {
                        // LD V0..Vx, [I]
                        for idx in 0..=x {
                            self.v[idx] = self.memory[self.i as usize + idx];
                        }
                    }

                    _ => {
                        // Unknown opcode
                    }
                }
            }

            _ => {
                // Unknown opcode
            }
        }
    }

    fn load_font(&mut self) {
        self.memory[FONT_START..FONT_START + FONT_BYTES].copy_from_slice(&FONT);
    }

    fn load_rom(&mut self, rom: &[u8]) {
        const START: usize = ROM_START;
        let end = START + rom.len();
        if end > self.memory.len() {
            eprintln!(
                "Error: ROM too large ({} bytes). Max allowed from 0x200 is {} bytes.",
                rom.len(),
                self.memory.len() - START
            );
            process::exit(1);
        }

        self.memory[START..end].copy_from_slice(rom);
    }
}

fn u8_to_0rgb(v: u8) -> u32 {
    // 0x00RRGGBB
    (v as u32) << 16 | (v as u32) << 8 | (v as u32)
}

fn parse_args() -> Vec<u8> {
    // return ROM data
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <path_to_rom>", args[0]);
        process::exit(1);
    }
    let rom_path = &args[1];
    let path = Path::new(rom_path);
    if !path.exists() {
        eprintln!("Error: ROM file '{}' does not exist.", rom_path);
        process::exit(1);
    }
    if !path.is_file() {
        eprintln!("Error: '{}' is not a file.", rom_path);
        process::exit(1);
    }
    let rom_data: Vec<u8> = match fs::read(path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to read ROM: {}", e);
            process::exit(1);
        }
    };
    return rom_data;
}

fn main() {
    // VM setup
    let mut vm: VM = VM::new();
    let rom_data: Vec<u8> = parse_args();
    vm.load_rom(&rom_data);
    vm.load_font();

    // Window setup
    let sdl_context = sdl3::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();

    let window = video_subsystem
        .window("chip8-emu-rs", WINDOW_WIDTH, WINDOW_HEIGHT)
        .position_centered()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas();
    // this allows to treat the canvas as FB_WIDTH x FB_WIDTH surface and then
    // SDL automatically scales it to the window resolution
    let _ = canvas.set_logical_size(
        FB_WIDTH,
        FB_WIDTH,
        sdl3_sys::render::SDL_RendererLogicalPresentation(1), //STRETCH
    );

    canvas.set_draw_color(Color::RGB(0, 222, 0));
    canvas.clear(); // this cloros the screen the color above
    canvas.present();

    // let mut fb_window: Vec<u32> = vec![0; WINDOW_WIDTH * WINDOW_HEIGHT];

    // // Audio setup

    // // timings
    // let cpu_hz = 600.0;
    // let cpu_dt = Duration::from_secs_f64(1.0 / cpu_hz);
    // let timer_dt = Duration::from_secs_f64(1.0 / 60.0);

    // let mut last = Instant::now();
    // let mut cpu_acc = Duration::ZERO;
    // let mut timer_acc = Duration::ZERO;
    // let mut frame_acc = Duration::ZERO;

    // while window.is_open() {
    //     let now = Instant::now();
    //     let dt = now - last;
    //     last = now;

    //     cpu_acc += dt;
    //     timer_acc += dt;
    //     frame_acc += dt;

    //     // run as many CPU cycles as needed
    //     while cpu_acc >= cpu_dt {
    //         vm.step();
    //         cpu_acc -= cpu_dt;
    //     }

    //     // timers at 60Hz
    //     while timer_acc >= timer_dt {
    //         vm.step_timers();
    //         timer_acc -= timer_dt;
    //     }

    //     // render at 60Hz
    //     while frame_acc >= timer_dt {
    //         if vm.draw_flag {
    //             scale_framebuffer(&vm.framebuffer, &mut fb_window);
    //             window
    //                 .update_with_buffer(&fb_window, WINDOW_WIDTH, WINDOW_HEIGHT)
    //                 .unwrap();
    //             vm.draw_flag = false;
    //         } else {
    //             window.update();
    //         }
    //         frame_acc -= timer_dt;
    //     }

    //     if vm.sound_timer > 0 {
    //         if sink.is_paused() {
    //             sink.play();
    //         }
    //     } else {
    //         if !sink.is_paused() {
    //             sink.pause();
    //         }
    //     }

    //     vm.keyboard = [false; 16];
    //     window.get_keys().iter().for_each(|key| match key {
    //         Key::Key1 => vm.keyboard[1] = true,
    //         Key::Key2 => vm.keyboard[2] = true,
    //         Key::Key3 => vm.keyboard[3] = true,
    //         Key::Q => vm.keyboard[4] = true,
    //         Key::W => vm.keyboard[5] = true,
    //         Key::E => vm.keyboard[6] = true,
    //         Key::A => vm.keyboard[7] = true,
    //         Key::S => vm.keyboard[8] = true,
    //         Key::D => vm.keyboard[9] = true,
    //         Key::Z => vm.keyboard[0xA] = true,
    //         Key::X => vm.keyboard[0x0] = true,
    //         Key::C => vm.keyboard[0xB] = true,
    //         Key::Key4 => vm.keyboard[0xC] = true,
    //         Key::R => vm.keyboard[0xD] = true,
    //         Key::F => vm.keyboard[0xE] = true,
    //         Key::V => vm.keyboard[0xF] = true,
    //         Key::Escape => process::exit(0),
    //         _ => (),
    //     });
    // }
}
