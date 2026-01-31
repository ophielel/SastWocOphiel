// src/tetris.rs
// SPDX-License-Identifier: GPL-2.0

//! Tetris game kernel module with character device interface

use kernel::{
    fs::{File, Kiocb},
    iov::{IovIterDest, IovIterSource},
    miscdevice::{MiscDevice, MiscDeviceOptions, MiscDeviceRegistration},
    prelude::*,
    sync::Arc,
    time,
    types::ForeignOwnable,
};

const BOARD_WIDTH: usize = 10;
const BOARD_HEIGHT: usize = 20;
const RENDER_BUFFER_SIZE: usize = 4096;

/// Ioctl command codes
const TETRIS_IOCTL_LEFT: u32 = 0x8000;
const TETRIS_IOCTL_RIGHT: u32 = 0x8001;
const TETRIS_IOCTL_DOWN: u32 = 0x8002;
const TETRIS_IOCTL_ROTATE: u32 = 0x8003;
const TETRIS_IOCTL_DROP: u32 = 0x8004;
const TETRIS_IOCTL_RESET: u32 = 0x8005;

/// Tetromino shapes (7 standard pieces)
#[derive(Debug, Clone, Copy, PartialEq)]
enum TetrominoType {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

/// Precomputed shape matrix for all rotations
#[derive(Debug, Clone, Copy)]
struct ShapeMatrix {
    rotations: [[[bool; 4]; 4]; 4],
}

impl ShapeMatrix {
    const fn from_base(base: [[bool; 4]; 4]) -> Self {
        let mut rotations = [[[false; 4]; 4]; 4];
        rotations[0] = base;
        rotations[1] = Self::rotate_once(base);
        rotations[2] = Self::rotate_once(rotations[1]);
        rotations[3] = Self::rotate_once(rotations[2]);
        Self { rotations }
    }

    const fn rotate_once(matrix: [[bool; 4]; 4]) -> [[bool; 4]; 4] {
        let mut rotated = [[false; 4]; 4];
        let mut i = 0;
        while i < 4 {
            let mut j = 0;
            while j < 4 {
                rotated[j][3 - i] = matrix[i][j];
                j += 1;
            }
            i += 1;
        }
        rotated
    }
}

/// Tetromino piece with position and rotation
#[derive(Debug, Clone, Copy)]
struct Tetromino {
    piece_type: TetrominoType,
    x: i32,
    y: i32,
    rotation: u8,
}

impl Tetromino {
    const SHAPES: [ShapeMatrix; 7] = [
        ShapeMatrix::from_base([
            [false, false, false, false],
            [true, true, true, true],
            [false, false, false, false],
            [false, false, false, false],
        ]),
        ShapeMatrix::from_base([
            [false, false, false, false],
            [false, true, true, false],
            [false, true, true, false],
            [false, false, false, false],
        ]),
        ShapeMatrix::from_base([
            [false, false, false, false],
            [false, true, false, false],
            [true, true, true, false],
            [false, false, false, false],
        ]),
        ShapeMatrix::from_base([
            [false, false, false, false],
            [false, true, true, false],
            [true, true, false, false],
            [false, false, false, false],
        ]),
        ShapeMatrix::from_base([
            [false, false, false, false],
            [true, true, false, false],
            [false, true, true, false],
            [false, false, false, false],
        ]),
        ShapeMatrix::from_base([
            [false, false, false, false],
            [true, false, false, false],
            [true, true, true, false],
            [false, false, false, false],
        ]),
        ShapeMatrix::from_base([
            [false, false, false, false],
            [false, false, true, false],
            [true, true, true, false],
            [false, false, false, false],
        ]),
    ];

    fn new(piece_type: TetrominoType) -> Self {
        Self {
            piece_type,
            x: (BOARD_WIDTH / 2) as i32 - 2,
            y: 0,
            rotation: 0,
        }
    }

    fn get_shape(&self) -> [[bool; 4]; 4] {
        let idx = match self.piece_type {
            TetrominoType::I => 0,
            TetrominoType::O => 1,
            TetrominoType::T => 2,
            TetrominoType::S => 3,
            TetrominoType::Z => 4,
            TetrominoType::J => 5,
            TetrominoType::L => 6,
        };
        Self::SHAPES[idx].rotations[(self.rotation % 4) as usize]
    }

    fn get_bounds(&self, shape: &[[bool; 4]; 4]) -> (i32, i32, i32, i32) {
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (4, 4, 0, 0);
        for i in 0..4 {
            for j in 0..4 {
                if shape[i][j] {
                    min_x = min_x.min(j as i32);
                    min_y = min_y.min(i as i32);
                    max_x = max_x.max(j as i32);
                    max_y = max_y.max(i as i32);
                }
            }
        }
        (min_x, min_y, max_x, max_y)
    }
}

/// Simple PRNG for kernel space
struct PRNG {
    state: u64,
}

impl PRNG {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(1),
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005);
        self.state = self.state.wrapping_add(1442695040888963407);
        self.state
    }

    fn next_range(&mut self, max: u32) -> u32 {
        (self.next() % max as u64) as u32
    }
}

/// Game state
struct TetrisGame {
    board: [[bool; BOARD_WIDTH]; BOARD_HEIGHT],
    current_piece: Option<Tetromino>,
    score: u32,
    game_over: bool,
    next_piece_type: TetrominoType,
    bag: [TetrominoType; 7],
    bag_idx: usize,
    prng: PRNG,
}

impl TetrisGame {
    fn new() -> Self {
        /*
         * Seed with a fast-changing clock value and mix in an address so that
         * successive opens aren't identical even if `ktime_get()` resolution is low.
         */
        let seed_time = <time::Monotonic as time::ClockSource>::ktime_get() as u64;
        let addr_mix = (&seed_time as *const u64 as usize) as u64;
        let prng = PRNG::new(seed_time ^ addr_mix ^ 0x2026);

        let mut game = Self {
            board: [[false; BOARD_WIDTH]; BOARD_HEIGHT],
            current_piece: None,
            score: 0,
            game_over: false,
            next_piece_type: TetrominoType::I,
            bag: [
                TetrominoType::I,
                TetrominoType::O,
                TetrominoType::T,
                TetrominoType::S,
                TetrominoType::Z,
                TetrominoType::J,
                TetrominoType::L,
            ],
            bag_idx: 7,
            prng,
        };

        game.next_piece_type = game.next_piece_from_bag();
        game
    }

    fn reset(&mut self) {
        self.board = [[false; BOARD_WIDTH]; BOARD_HEIGHT];
        self.current_piece = None;
        self.score = 0;
        self.game_over = false;
        self.spawn_piece();
    }

    fn spawn_piece(&mut self) {
        if self.game_over {
            return;
        }

        let new_piece = Tetromino::new(self.next_piece_type);

        if self.check_collision(&new_piece) {
            self.game_over = true;
            return;
        }

        self.current_piece = Some(new_piece);
        self.next_piece_type = self.next_piece_from_bag();
    }

    fn next_piece_from_bag(&mut self) -> TetrominoType {
        if self.bag_idx >= self.bag.len() {
            self.shuffle_bag();
            self.bag_idx = 0;
        }

        let piece = self.bag[self.bag_idx];
        self.bag_idx += 1;
        piece
    }

    fn shuffle_bag(&mut self) {
        /* Fisher-Yates shuffle. */
        let mut i = self.bag.len();
        while i > 1 {
            i -= 1;
            let j = self.prng.next_range((i + 1) as u32) as usize;
            let tmp = self.bag[i];
            self.bag[i] = self.bag[j];
            self.bag[j] = tmp;
        }
    }
}

impl TetrisGame {
    fn is_out_of_bounds(board_x: i32, board_y: i32) -> bool {
        board_x < 0
            || board_x >= BOARD_WIDTH as i32
            || board_y < 0
            || board_y >= BOARD_HEIGHT as i32
    }

    fn check_collision(&self, piece: &Tetromino) -> bool {
        let shape = piece.get_shape();
        let (min_x, min_y, max_x, max_y) = piece.get_bounds(&shape);

        for i in min_y..=max_y {
            for j in min_x..=max_x {
                if shape[i as usize][j as usize] {
                    let board_x = piece.x + j;
                    let board_y = piece.y + i;

                    if Self::is_out_of_bounds(board_x, board_y) {
                        return true;
                    }

                    if self.board[board_y as usize][board_x as usize] {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn move_left(&mut self) -> bool {
        if let Some(mut piece) = self.current_piece {
            piece.x -= 1;
            if !self.check_collision(&piece) {
                self.current_piece = Some(piece);
                return true;
            }
        }
        false
    }

    fn move_right(&mut self) -> bool {
        if let Some(mut piece) = self.current_piece {
            piece.x += 1;
            if !self.check_collision(&piece) {
                self.current_piece = Some(piece);
                return true;
            }
        }
        false
    }

    fn move_down(&mut self) -> bool {
        if let Some(mut piece) = self.current_piece {
            piece.y += 1;
            if !self.check_collision(&piece) {
                self.current_piece = Some(piece);
                return true;
            } else {
                self.lock_piece();
                return false;
            }
        }
        false
    }

    fn rotate(&mut self) -> bool {
        if let Some(mut piece) = self.current_piece {
            piece.rotation = (piece.rotation + 1) % 4;
            if !self.check_collision(&piece) {
                self.current_piece = Some(piece);
                return true;
            }
        }
        false
    }

    fn hard_drop(&mut self) {
        while self.move_down() {}
    }

    fn lock_piece(&mut self) {
        if let Some(piece) = self.current_piece.take() {
            let shape = piece.get_shape();
            let (min_x, min_y, max_x, max_y) = piece.get_bounds(&shape);

            for i in min_y..=max_y {
                for j in min_x..=max_x {
                    if shape[i as usize][j as usize] {
                        let board_x = piece.x + j;
                        let board_y = piece.y + i;

                        if !Self::is_out_of_bounds(board_x, board_y) {
                            self.board[board_y as usize][board_x as usize] = true;
                        }
                    }
                }
            }

            self.clear_lines();
            self.spawn_piece();
        }
    }

    fn clear_lines(&mut self) {
        let mut lines_cleared = 0;
        let mut write_idx = BOARD_HEIGHT;

        for y in (0..BOARD_HEIGHT).rev() {
            let line_full = (0..BOARD_WIDTH).all(|x| self.board[y][x]);

            if line_full {
                lines_cleared += 1;
            } else {
                write_idx -= 1;
                if write_idx != y {
                    self.board[write_idx] = self.board[y];
                }
            }
        }

        while write_idx > 0 {
            write_idx -= 1;
            self.board[write_idx] = [false; BOARD_WIDTH];
        }

        if lines_cleared > 0 {
            self.score += match lines_cleared {
                1 => 100,
                2 => 300,
                3 => 500,
                _ => 800,
            };
        }
    }

    fn render_to_buffer(&self, buffer: &mut [u8]) -> usize {
        let mut pos = 0;

        for i in 0..buffer.len() {
            buffer[i] = b' ';
        }

        let mut display_board = self.board;

        if let Some(piece) = self.current_piece {
            let shape = piece.get_shape();
            let (min_x, min_y, max_x, max_y) = piece.get_bounds(&shape);

            for i in min_y..=max_y {
                for j in min_x..=max_x {
                    if shape[i as usize][j as usize] {
                        let board_x = piece.x + j;
                        let board_y = piece.y + i;

                        if !Self::is_out_of_bounds(board_x, board_y) {
                            display_board[board_y as usize][board_x as usize] = true;
                        }
                    }
                }
            }
        }

        let top_border = b"\xE2\x95\x94";
        let horizontal = b"\xE2\x95\x90";
        let top_right = b"\xE2\x95\x97\n";

        pos += Self::write_bytes(buffer, pos, top_border);
        for _ in 0..BOARD_WIDTH {
            pos += Self::write_bytes(buffer, pos, horizontal);
            pos += Self::write_bytes(buffer, pos, horizontal);
        }
        pos += Self::write_bytes(buffer, pos, top_right);

        let left_border = b"\xE2\x95\x91";
        let right_border = b"\xE2\x95\x91\n";
        let filled = b"\xE2\x96\x88\xE2\x96\x88";
        let empty = b"  ";

        for row in &display_board {
            pos += Self::write_bytes(buffer, pos, left_border);
            for &cell in row {
                let bytes: &[u8] = if cell { filled } else { empty };
                pos += Self::write_bytes(buffer, pos, bytes);
            }
            pos += Self::write_bytes(buffer, pos, right_border);
        }

        let bottom_left = b"\xE2\x95\x9A";
        let bottom_right = b"\xE2\x95\x9D\n";

        pos += Self::write_bytes(buffer, pos, bottom_left);
        for _ in 0..BOARD_WIDTH {
            pos += Self::write_bytes(buffer, pos, horizontal);
            pos += Self::write_bytes(buffer, pos, horizontal);
        }
        pos += Self::write_bytes(buffer, pos, bottom_right);

        pos += Self::write_bytes(buffer, pos, b"Score: ");
        pos += Self::write_number(buffer, pos, self.score);
        pos += Self::write_bytes(buffer, pos, b"\n");

        if self.game_over {
            pos += Self::write_bytes(buffer, pos, b"GAME OVER!\n");
        }

        pos
    }

    fn write_bytes(buffer: &mut [u8], pos: usize, bytes: &[u8]) -> usize {
        let mut written = 0;
        for &byte in bytes {
            if pos + written < buffer.len() {
                buffer[pos + written] = byte;
                written += 1;
            } else {
                break;
            }
        }
        written
    }

    fn write_number(buffer: &mut [u8], pos: usize, mut num: u32) -> usize {
        let mut digits = [0u8; 10];
        let mut digit_count = 0;

        if num == 0 {
            digits[0] = b'0';
            digit_count = 1;
        } else {
            while num > 0 && digit_count < 10 {
                digits[digit_count] = (num % 10) as u8 + b'0';
                num /= 10;
                digit_count += 1;
            }
        }

        let mut written = 0;
        for i in (0..digit_count).rev() {
            if pos + written < buffer.len() {
                buffer[pos + written] = digits[i];
                written += 1;
            }
        }
        written
    }
}

/// Device state
pub(crate) struct TetrisDevice {
    inner: Arc<TetrisDeviceInner>,
}

#[pin_data]
struct TetrisDeviceInner {
    #[pin]
    game: kernel::sync::Mutex<TetrisGame>,
}

impl TetrisDevice {
    fn new() -> Result<Arc<Self>> {
        let inner = Arc::pin_init(
            pin_init!(TetrisDeviceInner {
                game <- kernel::new_mutex!(TetrisGame::new()),
            }),
            GFP_KERNEL,
        )?;

        inner.game.lock().spawn_piece();

        Ok(Arc::new(Self { inner }, GFP_KERNEL)?)
    }
}

#[vtable]
impl MiscDevice for TetrisDevice {
    type Ptr = Arc<TetrisDevice>;

    fn open(_file: &File, _misc: &MiscDeviceRegistration<Self>) -> Result<Self::Ptr> {
        TetrisDevice::new()
    }

    fn read_iter(kiocb: Kiocb<'_, Self::Ptr>, iov: &mut IovIterDest<'_>) -> Result<usize> {
        let device = kiocb.file();
        let game = device.inner.game.lock();

        let mut buffer = kernel::alloc::KVec::new();
        buffer.resize(RENDER_BUFFER_SIZE, 0, GFP_KERNEL)?;

        let len = game.render_to_buffer(&mut buffer);

        let bytes_to_copy = core::cmp::min(len, iov.len());
        let copied = iov.copy_to_iter(&buffer[..bytes_to_copy]);

        drop(game);

        Ok(copied)
    }

    fn write_iter(kiocb: Kiocb<'_, Self::Ptr>, iov: &mut IovIterSource<'_>) -> Result<usize> {
        let device = kiocb.file();
        let mut buffer = [0u8; 1];
        let len = iov.copy_from_iter(&mut buffer);

        if len > 0 {
            let mut game = device.inner.game.lock();
            match buffer[0] {
                b'a' | b'A' => {
                    game.move_left();
                }
                b'd' | b'D' => {
                    game.move_right();
                }
                b's' | b'S' => {
                    game.move_down();
                }
                b'w' | b'W' => {
                    game.rotate();
                }
                b' ' => {
                    game.hard_drop();
                }
                b'r' | b'R' => {
                    game.reset();
                }
                _ => {}
            }
        }

        Ok(len)
    }

    fn ioctl(
        device: <Self::Ptr as ForeignOwnable>::Borrowed<'_>,
        _file: &File,
        cmd: u32,
        _arg: usize,
    ) -> Result<isize> {
        let mut game = device.inner.game.lock();

        match cmd {
            TETRIS_IOCTL_LEFT => {
                game.move_left();
            }
            TETRIS_IOCTL_RIGHT => {
                game.move_right();
            }
            TETRIS_IOCTL_DOWN => {
                game.move_down();
            }
            TETRIS_IOCTL_ROTATE => {
                game.rotate();
            }
            TETRIS_IOCTL_DROP => {
                game.hard_drop();
            }
            TETRIS_IOCTL_RESET => {
                game.reset();
            }
            _ => return Err(EINVAL),
        }

        Ok(0)
    }
}

pub(crate) fn register_tetris_device(
) -> Result<Pin<kernel::alloc::KBox<MiscDeviceRegistration<TetrisDevice>>>> {
    kernel::alloc::KBox::pin_init(
        MiscDeviceRegistration::register(MiscDeviceOptions { name: c"tetris" }),
        GFP_KERNEL,
    )
}
