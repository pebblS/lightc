use crossterm::queue;
use crossterm::terminal::{enable_raw_mode, disable_raw_mode};
use crossterm::{
    cursor,
    event::{read, Event, KeyCode, KeyEvent},
    style::Print,
    terminal::{self, Clear, ClearType},
};
use std::io::{stdout, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq, Clone)]
enum Lcmd {
    Conn,
    Dc,
    Say,
    Nick,
    Quit,
}

#[derive(Debug, Clone)]
struct Lcommand {
    cmd_type: Lcmd,
    user: String,
    content: String,
}

impl Lcommand {
    // Construct command from user input
    /*
    fn new(buf: String, user: String) -> Lcommand {
        let cmd_split: Vec<&str> = buf.split(' ').collect();
        //dbg!(cmd_split[0]);
        let cmd_type = match cmd_split[0] {
            "/connect" => Lcmd::Conn,
            "/disconnect\n" => Lcmd::Dc,
            _ => Lcmd::Say,
        };
        let mut content = match cmd_type {
            Lcmd::Say => cmd_split.join(" "),
            _ => cmd_split[1..].join(" "),
        };

        if !content.is_empty() {
            content = content.as_str()[0..content.len() - 1].to_string();
        }
        //dbg!(&content);
        //dbg!(&cmd_type);
        //dbg!(&user);
        //dbg!(&content);
        Lcommand {
            cmd_type,
            user,
            content,
        }
    }
    */

    fn display(self, from_client: bool) -> String {
        let mut output = String::new();
        if !from_client {
            match self.cmd_type {
                Lcmd::Say => output.push_str(format!("<{}>: {}", self.user, self.content).as_str()),
                Lcmd::Conn => output.push_str(format!("[SERVER]: {} joined", self.user).as_str()),
                Lcmd::Dc => output.push_str(format!("[SERVER]: {} left", self.user).as_str()),
                Lcmd::Nick => output.push_str(
                    format!(
                        "[SERVER]: {} changed their nickname to {}",
                        self.user, self.content
                    )
                    .as_str(),
                ),
                _ => (),
            }
            output
        } else {
            match self.cmd_type {
                Lcmd::Say => output.push_str(format!("<{}>: {}", self.user, self.content).as_str()),
                Lcmd::Conn => output.push_str("[CLIENT]: you joined"),
                Lcmd::Dc => output.push_str("[CLIENT]: you left"),
                _ => (),
            }
            output
        }
    }

    fn from(buf: String) -> Lcommand {
        let cmd_split: Vec<&str> = buf.split('\n').collect();
        //dbg!(cmd_split.get(0));
        //dbg!(cmd_split.get(1));
        //dbg!(cmd_split.get(2));
        let cmd_type = match cmd_split[0] {
            "SAY" => Lcmd::Say,
            "CONNECT" => Lcmd::Conn,
            "DISCONNECT" => Lcmd::Dc,
            _ => panic!("should not be reachable"),
        };
        let user = String::from(cmd_split[1]);
        let content = match cmd_type {
            Lcmd::Say => String::from(cmd_split[2]),
            _ => String::new(),
        };

        Lcommand {
            cmd_type,
            user,
            content,
        }
    }
}

struct Client {
    username: String,
    connected: Option<String>,
    tx: Option<mpsc::Sender<Lcommand>>, // Channel to send messages to connected server
    rx: Option<mpsc::Receiver<Lcommand>>, // Channel to receive messages from connected server
    messages: Vec<String>,
    user_in: mpsc::Receiver<char>,
}

impl Client {
    fn new(user: String) -> Client {
        let user_tx: mpsc::Sender<char>;
        let user_in: mpsc::Receiver<char>;
        let channel = mpsc::channel();
        user_tx = channel.0;
        user_in = channel.1;

        // User input monitor thread
        thread::spawn(move || loop {
            let k = read().unwrap();
            match k {
                // entered random character
                Event::Key(KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers: _m,
                }) => user_tx.send(c).unwrap(),

                // backspace
                Event::Key(KeyEvent {
                    code: KeyCode::Backspace,
                    modifiers: _m,
                }) => user_tx.send(0x8 as char).unwrap(), // Backspace ascii Pog

                Event::Key(KeyEvent {
                    code: KeyCode::Enter,
                    modifiers: _m,
                }) => user_tx.send(0xA as char).unwrap(), // Newline ascii Pog

                _ => (), // Ignore other events
            }
        });

        Client {
            username: user,
            connected: None,
            tx: None,
            rx: None,
            messages: vec![],
            user_in,
        }
    }

    // returns true if connected to a server, false if not
    fn send_msg(&mut self, msg: Lcommand) -> bool {
        let mut msg = msg;
        if msg.cmd_type == Lcmd::Nick {
        } else {
            msg.user = self.username.clone();
        } 
        if msg.cmd_type == Lcmd::Dc {
            self.connected = None;
        }
        //self.tx.as_ref().unwrap().send(msg).unwrap();
        if self.tx.is_some() {
            let tx = self.tx.as_ref().unwrap();
            let ret = tx.send(msg);
            if ret.is_err() {
                return false;
            }
            true
        } else {
            false
        }
    }

    fn connect(&mut self, addr: String) {
        let tx: mpsc::Sender<Lcommand>;
        let out_rx: mpsc::Receiver<Lcommand>;
        let channel = mpsc::channel();
        tx = channel.0;
        out_rx = channel.1;
        let out_stream = TcpStream::connect(addr.clone());
        if out_stream.is_err() {
            self.messages.push(format!("[CLIENT]: failed to join {}", addr));
            return
        }
        let mut out_stream = out_stream.unwrap();
        self.messages.push(format!("[CLIENT]: you joined {}", &addr));
        self.connected = Some(addr);
        let mut rec_stream = out_stream.try_clone().unwrap();

        // Output thread
        std::thread::spawn(move || {
            loop {
                let mut end = false;
                let msg = out_rx.recv().unwrap();
                let mut out_buf = String::new();
                match msg.cmd_type {
                    Lcmd::Conn => out_buf.push_str("CONNECT\n"),
                    Lcmd::Dc => {
                        out_buf.push_str("DISCONNECT\n");
                        end = true // Stop handling the stream when Dc is passed
                    }
                    Lcmd::Say => out_buf.push_str("SAY\n"),
                    Lcmd::Nick => out_buf.push_str("NICK\n"),
                    _ => (),
                }
                out_buf.push_str(&msg.user);
                out_buf.push('\n');
                out_buf.push_str(&msg.content);
                out_buf.push('\n');
                let _n = out_stream.write(out_buf.as_bytes()).unwrap();
                if end {
                    break;
                }
            }
            out_stream.shutdown(std::net::Shutdown::Both).unwrap();
        });

        let rx: mpsc::Receiver<Lcommand>;
        let in_tx: mpsc::Sender<Lcommand>;
        let channel = mpsc::channel();
        in_tx = channel.0;
        rx = channel.1;
        // Receiver thread
        std::thread::spawn(move || {
            let mut msgbuf: Vec<u8> = vec![0; 1024];
            loop {
                let n = rec_stream.read(&mut msgbuf);
                if n.is_err() {
                    break;
                }
                in_tx
                    .send(Lcommand::from(String::from_utf8(msgbuf.clone()).unwrap()))
                    .unwrap();
            }
        });
        self.tx = Some(tx);
        self.rx = Some(rx)
    }

    fn display_messages(&self, mut out: &std::io::Stdout) {
        if !self.messages.is_empty() {
            let mut msg_iter = self.messages.clone();
            msg_iter.reverse();
            let mut msg_iter = msg_iter.into_iter();

            // Print messages
            queue!(out, cursor::MoveToRow(terminal::size().unwrap().1 - 1)).unwrap();
            for _ in 0..terminal::size().unwrap().1 {
                let msg = msg_iter.next();
                if msg.is_some() {
                    queue!(out, cursor::MoveToPreviousLine(1), Print(msg.unwrap())).unwrap();
                } else {
                    break;
                }
            }
        }
    }

    fn print_prompt(&self, mut out: &std::io::Stdout, text: String) {
        let status: String;
        if self.connected.is_some() {
            status = format!("[{}]", self.connected.as_ref().unwrap());
        } else {
            status = String::from("[]");
        }
        queue!(
            out,
            cursor::MoveTo(0, terminal::size().unwrap().1),
            Print(format!("{}: {}", status, text)),
        )
        .unwrap();
    }

    fn print_welcome(&mut self) {
        self.messages.push("Hi! Welcome to LightC! :)".to_string());
        self.messages.push("Set your nickname with '/nick ...'".to_string());
        self.messages.push("Connect to a server with '/connect ...'".to_string());
    }
}

fn main() {
    let mut client = Client::new(String::from("test_user"));
    //client.connect(String::from("127.0.0.1:6969"));
    let mut prompt_text = String::new();
    let mut stdout = stdout();
    enable_raw_mode().unwrap();
    crossterm::queue!(stdout, crossterm::terminal::Clear(ClearType::All)).unwrap();
    
    client.print_welcome();

    loop {
        // Get new received messages
        if client.rx.is_some() {
            let new_msg = client.rx.as_ref().unwrap().try_recv();
            if let Ok(new_msg) = new_msg {
                client.messages.push(new_msg.display(false));
            }
        }

        clear_screen(&stdout);

        client.display_messages(&stdout);

        let mut cmd: Option<Lcommand> = None;

        let received_char = client.user_in.try_recv();
        if let Ok(character) = received_char {
            if character == 0xA as char {
                // newline
                cmd = Some(parse_cmd(prompt_text.clone(), &mut client));
                prompt_text.clear();
            } else if character == 0x8 as char {
                // backspace
                prompt_text.pop();
            } else {
                prompt_text.push(character);
            }
        }
        client.print_prompt(&stdout, prompt_text.clone());
        // Send command
        if let Some(command) = cmd {
            if command.cmd_type == Lcmd::Quit {
                break;
            }
            else if command.cmd_type == Lcmd::Conn {
                client.connect(command.content);
            } else {
                let success = client.send_msg(command.clone());
                if success {
                    client.messages.push(command.clone().display(true));
                } else {
                    client.connected = None;
                    client.messages.push("[CLIENT]: not connected to a server".to_string());
                }
            }
        }

        

        stdout.flush().unwrap();
        std::thread::sleep(Duration::from_millis(11));
    }

    // Make terminal normal again
    disable_raw_mode().unwrap();
}

fn clear_screen(mut out: &std::io::Stdout) {
    // Clear screen
    queue!(
        out,
        cursor::MoveTo(0, 0),
        Clear(ClearType::All)
    )
    .unwrap();
}

fn parse_cmd(buf: String, client: &mut Client) -> Lcommand {
    let cmd_split: Vec<&str> = buf.split(' ').collect();
    //dbg!(cmd_split[0]);
    let cmd_type = match cmd_split[0] {
        "/connect" => Lcmd::Conn,
        "/disconnect" => Lcmd::Dc,
        "/nick" => Lcmd::Nick,
        "/quit" => Lcmd::Quit,
        _ => Lcmd::Say,
    };
    let content = match cmd_type {
        Lcmd::Say => cmd_split.join(" "),
        _ => cmd_split[1..].join(" "),
    };

    if cmd_type == Lcmd::Nick {
        let old_username = client.username.clone();
        client.username = content.clone();
        client.messages.push(format!("[CLIENT]: you changed your nickname to {}", client.username.clone()));
        return Lcommand {
            cmd_type,
            user: old_username,
            content,
        };
    }
    Lcommand {
        cmd_type,
        user: client.username.clone(),
        content,
    }
}
