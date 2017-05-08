extern crate byteorder;

use std::io::Cursor;
use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};
use std::io::prelude::*;
use std::net::TcpStream;

#[derive(Debug)]
enum Interpolation {
    Step   = 0,
    Linear = 1,
    Smooth = 2,
    Ramp   = 3,
}

impl From<u8> for Interpolation {
    fn from(raw: u8) -> Interpolation {
        match raw {
            0 => Interpolation::Step,
            1 => Interpolation::Linear,
            2 => Interpolation::Smooth,
            3 => Interpolation::Ramp,
            _ => Interpolation::Step,
        }
    }
}

#[derive(Debug)]
struct Key {
    row: u32,
    value: f32,
    interpolation: Interpolation,
}

#[derive(Debug)]
struct Track {
    name: String,
    keys: Vec<Key>,
}

impl Track {
    fn new<S: Into<String>>(name: S) -> Track {
        Track {
            name: name.into(),
            keys: Vec::new(),
        }
    }

    //TODO set_key(&mut self, key: Key)
    //TODO delete_key(&mut self, row: u32)
    //TODO get_value(row: f32) -> f32
    //  This should work with half-rows
}

#[derive(Copy, Clone, Debug)]
struct RocketErr {
}

enum RocketState {
    NewCommand,
    IncompleteCommand(usize),
    CompleteCommand,
}

struct Rocket {
    stream: TcpStream,
    state: RocketState,
    cmd: Vec<u8>,
    tracks: Vec<Track>,
    row: u32,
    paused: bool,
}

impl Rocket {
    pub fn new() -> Result<Rocket, RocketErr> {
        Rocket::connect("localhost", 1338)
    }

    pub fn connect(host: &str, port: u16) -> Result<Rocket, RocketErr> {
        let stream = TcpStream::connect((host, port)).expect("Failed to connect");

        let mut rocket = Rocket {
            stream: stream,
            state: RocketState::NewCommand,
            cmd: Vec::new(),
            tracks: Vec::new(),
            row: 0,
            paused: true,
        };

        rocket.handshake().expect("Failed to handshake");

        rocket.stream.set_nonblocking(true).unwrap();

        Ok(rocket)
    }

    pub fn get_row(&self) -> u32 {
        self.row
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn get_track(&mut self, name: &str) -> &Track {
        if !self.tracks.iter().any(|t| t.name == name) {

            // Send GET_TRACK message
            let mut buf = vec![2];
            buf.write_u32::<BigEndian>(name.len() as u32).unwrap();
            buf.extend_from_slice(&name.as_bytes());
            self.stream.write(&buf).unwrap();

            self.tracks.push(Track::new(name));
        }
        self.tracks.iter().find(|t| t.name == name).unwrap()
    }

    pub fn set_row(&mut self, row: u32) {
        self.row = row;

        // Send SET_ROW message
        let mut buf = vec![3];
        buf.write_u32::<BigEndian>(row).unwrap();
        self.stream.write(&buf).unwrap();
    }

    pub fn poll_events(&mut self) {
        match self.state {
            RocketState::NewCommand => {
                let mut buf = [0;1];
                if let Ok(_) = self.stream.read_exact(&mut buf) {
                    self.cmd.extend_from_slice(&buf);
                    match self.cmd[0] {
                        0 => self.state = RocketState::IncompleteCommand(4+4+4+1), //SET_KEY
                        1 => self.state = RocketState::IncompleteCommand(4+4), //DELETE_KEY
                        3 => self.state = RocketState::IncompleteCommand(4), //SET_ROW
                        4 => self.state = RocketState::IncompleteCommand(1), //PAUSE
                        5 => self.state = RocketState::CompleteCommand, //SAVE_TRACKS
                        _ => self.state = RocketState::CompleteCommand,
                    }
                }
            },
            RocketState::IncompleteCommand(bytes) => {
                let mut buf = vec![0;bytes];
                if let Ok(bytes_read) = self.stream.read(&mut buf) {
                    self.cmd.extend_from_slice(&buf);
                    if bytes-bytes_read > 0 {
                        self.state = RocketState::IncompleteCommand(bytes-bytes_read);
                    } else {
                        self.state = RocketState::CompleteCommand;
                    }
                }
            },
            RocketState::CompleteCommand => {
                {
                    let mut cursor = Cursor::new(&self.cmd);
                    let cmd = cursor.read_u8().unwrap();
                    match cmd {
                        0 => {
                            let track = cursor.read_u32::<BigEndian>().unwrap();
                            let row = cursor.read_u32::<BigEndian>().unwrap();
                            let value = cursor.read_f32::<BigEndian>().unwrap();
                            //let interpolation = cursor.read_u8().unwrap();
                                // 0 == step
                                // 1 == linear
                                // 2 == smooth
                                // 3 == ramp
                            let interpolation = Interpolation::from(cursor.read_u8().unwrap());
                            let key = Key {
                                row: row,
                                value: value,
                                interpolation: interpolation,
                            };
                            println!("SET_KEY (track: {:?}) (key: {:?})", track, key);
                        },
                        1 => {
                            let track = cursor.read_u32::<BigEndian>().unwrap();
                            let row = cursor.read_u32::<BigEndian>().unwrap();
                            println!("DELETE_KEY (track: {:?}) (row: {:?})", track, row);
                        },
                        3 => {
                            let row = cursor.read_u32::<BigEndian>().unwrap();
                            println!("SET_ROW (row: {:?})", row);

                            self.row = row;
                        },
                        4 => {
                            let flag = cursor.read_u8().unwrap();
                                // 0 or 1
                            println!("PAUSE {:?}", flag);

                            self.paused = flag == 1;
                        },
                        5 => {
                            println!("SAVE_TRACKS");
                        },
                        _ => println!("Unknown {:?}", cmd),
                    }
                }

                self.cmd.clear();
                self.state = RocketState::NewCommand;
            },
        }
    }

    fn handshake(&mut self) -> Result<(), RocketErr> {
        let client_greeting = "hello, synctracker!";
        let server_greeting = "hello, demo!";

        self.stream.write(client_greeting.as_bytes()).expect("Failed to write client greeting");
        let mut buf = [0;12];
        self.stream.read_exact(&mut buf).expect("Failed to read server greeting");
        let read_greeting = std::str::from_utf8(&buf).expect("Failed to convert buf to utf8");
        if read_greeting == server_greeting {
            Ok(())
        } else {
            Err(RocketErr{})
        }
    }
}

fn main() {
    let mut rocket = Rocket::new().unwrap();
    rocket.get_track("test");
    rocket.get_track("test2");
    rocket.get_track("a:test2");
    rocket.set_row(5);

    loop {
        rocket.poll_events();
        std::thread::sleep_ms(1);
    }
}
