use serenity::{
    framework::standard::{
        help_commands,
        macros::{check, command, group, help},
        Args, CheckResult, CommandGroup, CommandOptions, CommandResult, DispatchError, HelpOptions,
        StandardFramework,
    },
    model::{channel::{Embed, Message}, gateway::Ready},
    prelude::*,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::net::TcpStream;
use std::io::{Read, Write, BufRead, BufReader};
use serde_json::json;

struct Handler;

impl EventHandler for Handler {
    fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

struct Team {
    tees: Vec<String>,
}

struct TeamList;
impl TypeMapKey for TeamList {
    type Value = HashMap<String, Team>;
}

struct Server {
    socket: TcpStream,
}
struct ServerList;
impl TypeMapKey for ServerList {
    type Value = Vec<Server>;
}

#[group]
#[prefix("server")]
#[commands(server_add)]
struct ServerCmd;

#[command]
#[aliases("add")]
fn server_add(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
    let server_addr = args.single::<String>()?;
    let password = args.single::<String>()?;

    dbg!(&server_addr);

    let mut ec_stream = TcpStream::connect(server_addr);

    match ec_stream {
        Ok(mut stream) => {
            loop {
                let mut stream_reader = BufReader::new(&stream);
                let mut buf = Vec::new();
                match stream_reader.read_until(b'\0', &mut buf) {
                    Ok(0) => { }
                    Ok(num) => {
                        let line = std::str::from_utf8(&buf);
                        match line {
                            Ok(string) => {
                                match string.trim_end_matches(&['\r', '\n', '\0'][..]) {
                                    "Enter password:" => {
                                        let auth = format!("{}\r\n", password);
                                        stream.write(auth.as_bytes());
                                    }
                                    "Authentication successful. External console access granted." => {
                                        let mut data = ctx.data.write();
                                        let servers = data.get_mut::<ServerList>().expect("Expected ServerList in ShareMap.");
                                        servers.push(Server { socket: stream });
                                        msg.channel_id.say(&ctx.http, &format!("server authed & added"));
                                        break;
                                    }
                                    text => {
                                        msg.channel_id.say(&ctx.http, &format!("<- {:?}", text));
                                        break;
                                    }
                                }

                            }
                            _ => { }
                        }
                    }
                    Err(err) => { dbg!(err); }
                }
            }
        },
        Err(err) => {
            if let Err(why) = msg.channel_id.say(&ctx.http, &format!("failed to connect {:?}", err)) {
                println!("Error sending message: {:?}", why);
            }
        }
    }

    Ok(())
}

#[group]
#[prefix("team")]
#[commands(team_add, team_list)]
struct TeamCmd;

#[command]
#[aliases("add")]
fn team_add(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
    let team_name = args.single::<String>()?;

    let mut team = Team { tees: Vec::new() };
    for tee in &msg.mentions {
        if tee.bot {
            continue;
        }
        team.tees.push(tee.name.clone());
    }

    if team.tees.is_empty() {
        if let Err(why) = msg.channel_id.say(&ctx.http, "can't add team with 0 members") {
            println!("Error sending message: {:?}", why);
        }
        return Ok(());
    }

    let mut data = ctx.data.write();
    let teams = data.get_mut::<TeamList>().expect("Expected TeamList in ShareMap.");
    teams.entry(team_name.clone()).or_insert(team);

    if let Err(why) = msg.channel_id.say(&ctx.http, &format!("added team {}", &team_name)) {
        println!("Error sending message: {:?}", why);
    }

    Ok(())
}

#[command]
#[aliases("list")]
fn team_list(ctx: &mut Context, msg: &Message) -> CommandResult {
    let data = ctx.data.read();
    let teams = data.get::<TeamList>().expect("Expected TeamList in ShareMap.");

    let webhook_id = env::var("DISCORD_HOOK_ID").expect("Expected a webhook id in the environment").parse::<u64>().unwrap_or(0);
    let webhook_token = env::var("DISCORD_HOOK_TOKEN").expect("Expected a webhook token in the environment");
    let webhook = ctx.http.get_webhook_with_token(webhook_id, &webhook_token).unwrap();

    let embed_teams = Embed::fake(|e| {
        let mut e = e.title("Teams")
            .description("Currently registered teams");
        for (i, (name, team)) in teams.iter().enumerate() {
            let mut tees = String::new();
            for tee in &team.tees {
                tees += tee;
                tees += " ";
            }
            e = e.field(name, tees, false);
        }
        e
    });

    let _ = webhook.execute(&ctx.http, false, |mut w| {
        w.embeds(vec![embed_teams]);
        w
    });

    Ok(())
}

fn main() {
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let mut client = Client::new(&token, Handler).expect("Err creating client");

    {
        let mut data = client.data.write();
        data.insert::<TeamList>(HashMap::default());
        data.insert::<ServerList>(Vec::new());
    }

    let (owners, bot_id) = match client.cache_and_http.http.get_current_application_info() {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    client.with_framework(
        StandardFramework::new()
            .configure(|c| c.with_whitespace(true).on_mention(Some(bot_id)))
            .group(&TEAMCMD_GROUP)
            .group(&SERVERCMD_GROUP)
    );

    if let Err(why) = client.start() {
        println!("An error occurred while running the client: {:?}", why);
    }
}