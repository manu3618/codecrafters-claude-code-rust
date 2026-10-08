use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use codecrafters_claude_code::skill;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::any::Any;
use std::collections::HashMap;
use std::fs;
use std::fs::read_to_string;
use std::path;
use std::process::Command;
use std::{env, process};

const MAX_LOOP: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
enum Tool {
    Read,
    Write,
    Bash,
    Skill,
}

impl Tool {
    fn to_spec(&self) -> Value {
        match &self {
            Self::Read => json!({
                "type": "function",
                "function": {
                    "name": "Read",
                    "description": "Read and return the contents of a file",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "The path to the file to read"
                            }
                        },
                        "required": ["file_path"]
                    }
                }
            }),
            Self::Write => json!({
                  "type": "function",
                  "function": {
                      "name": "Write",
                      "description": "Write content to a file",
                      "parameters": {
                          "type": "object",
                          "required": ["file_path", "content"],
                          "properties": {
                              "file_path": {
                                  "type": "string",
                                  "description": "The path of the file to write to"
                              },
                              "content": {
                                  "type": "string",
                                  "description": "The content to write to the file"
                              }
                          }
                      }
                  }
            }),
            Self::Bash => json!({
                "type": "function",
                "function": {
                    "name": "Bash",
                    "description": "Execute a shell command",
                    "parameters": {
                        "type": "object",
                        "required": ["command"],
                        "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute"
                        }
                        }
                    }
                }
            }),
            Self::Skill => json!({
                "type": "function",
                "function": {
                    "name": "Skill",
                    "description": "Load a skill's instructions into the conversation",
                    "parameters": {
                        "type": "object",
                        "required": ["name"],
                        "properties": {
                            "name": { "type": "string", "description": "The name of the skill to use" },
                            "args": { "type": "string", "description": "Optional arguments for the skill" }
                        }
                    }
                }
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionCall {
    name: Tool,
    // arguments: HashMap<String, String>,
    arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    /// list of active skills
    #[serde(skip_serializing, skip_deserializing)]
    active_skills: Vec<skill::Skill>,
    /// config used for running agent, if any
    #[serde(skip_serializing, skip_deserializing)]
    agent_config: Option<OpenAIConfig>,
}

impl FunctionCall {
    /// Execute a tool call
    fn execute(&self) -> String {
        match self.name {
            Tool::Read => self.read(),
            Tool::Write => self.write(),
            Tool::Bash => self.bash(),
            Tool::Skill => self.skill(),
        }
    }
    fn read(&self) -> String {
        let arguments: HashMap<String, String> = serde_json::from_str(&self.arguments).unwrap();
        dbg!(&arguments);
        let file_path = arguments.get("file_path").unwrap();
        read_to_string(file_path).unwrap()
    }
    fn write(&self) -> String {
        let arguments: HashMap<String, String> = serde_json::from_str(&self.arguments).unwrap();
        dbg!(&arguments);
        let file_path = arguments.get("file_path").unwrap();
        let content = arguments.get("content").unwrap();
        match fs::write(file_path, content) {
            Ok(_) => "file written succesfully".into(),
            Err(e) => format!("Error creating file: {e:?}"),
        }
    }
    fn bash(&self) -> String {
        let arguments: HashMap<String, String> = serde_json::from_str(&self.arguments).unwrap();
        dbg!(&arguments);
        let command = arguments.get("command").unwrap();
        dbg!(&command);
        let cmd = Command::new("bash")
            .arg("-c")
            .arg(command)
            .output()
            .unwrap();
        format!(
            "{}{}",
            String::from_utf8(cmd.stdout).unwrap(),
            String::from_utf8(cmd.stderr).unwrap()
        )
    }
    fn skill(&self) -> String {
        let arguments: HashMap<String, String> = serde_json::from_str(&self.arguments).unwrap();
        dbg!(&arguments);
        let name = arguments.get("name").unwrap();
        let b = String::from("");
        let args = arguments.get("args").unwrap_or(&b);
        let selected_skill = self
            .active_skills
            .iter()
            .filter(|s| s.frontmatter.name == *name)
            .next()
            .unwrap();
        let body = selected_skill.get_bundled_body(&args.clone());
        if selected_skill.frontmatter.context.is_none() {
            return body;
        }

        // TODO:
        // 1. create an (sub)agent
        // 2. give skill body as first message
        // 3. get subagent's last answer and return it
        dbg!(selected_skill);
        dbg!(&self.agent_config);
        todo!("subagent")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    #[default]
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Conversation {
    role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
}

impl Conversation {
    fn from_skills(skills: &[skill::Skill]) -> Self {
        let mut content = Vec::new();
        content.push(String::from("You have access to the following skills:\n\n"));
        content.append(
            &mut skills
                .iter()
                .map(|s| format!("- {}: {}", s.frontmatter.name, s.frontmatter.description))
                .collect(),
        );
        content.push(
            concat!(
                "\nIf a skill matches the user's request, call the Skill tool ",
                "with its name and follow the instructions it returns."
            )
            .to_string(),
        );
        Self {
            role: Role::System,
            content: Some(content.join("\n")),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ConversationHistory(Vec<Conversation>);

impl ConversationHistory {
    fn add_response(&mut self, conv: &str) {
        let response: Conversation = serde_json::from_str(conv).unwrap();
        self.0.push(response);
    }
    fn to_spec(&self) -> Value {
        json!(self.0)
    }
}

#[derive(Default)]
struct Agent<C: async_openai::config::Config> {
    client: Client<C>,
    skills: Vec<skill::Skill>,
    conversation_history: ConversationHistory,
}

impl<C> Agent<C>
where
    C: async_openai::config::Config + 'static,
{
    fn with_config(&self, config: C) -> Self {
        let client = Client::with_config(config);
        let skills = skill::get_skills(path::Path::new(".claude/skills"));
        let skill_message = Conversation::from_skills(&skills);
        let mut conversation_history = self.conversation_history.clone();
        conversation_history.0.push(skill_message);
        Self {
            client,
            skills,
            conversation_history,
        }
    }

    /// Add user message to conversation
    fn add_user_message(&mut self, args: String) {
        let mut active_skills = Vec::new();
        let mut arguments = args.split(' ');
        let mut skill_arguments = String::new();
        while let Some(c) = arguments.next() {
            if let Some(name) = c.strip_prefix("/") {
                active_skills.push(
                    self.skills
                        .iter()
                        .find(|s| s.frontmatter.name == name)
                        .unwrap(),
                )
            } else {
                skill_arguments =
                    format!("{c} ") + &arguments.clone().collect::<Vec<_>>().join(" ");
                break;
            }
        }
        if active_skills.is_empty() {
            self.conversation_history.0.push(Conversation {
                role: Role::User,
                content: Some(skill_arguments.clone()),
                ..Default::default()
            })
        } else {
            for skill in &active_skills {
                self.conversation_history.0.push(Conversation {
                    role: Role::User,
                    content: Some(skill.get_bundled_body(&skill_arguments.clone())),
                    ..Default::default()
                })
            }
        }
    }

    /// Run agentic loop updating conversation.
    async fn run_agent_loop(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        #[allow(unused_variables)]
        let read_tool = Tool::Read;
        let write_tool = Tool::Write;
        let bash_tool = Tool::Bash;
        let skill_tool = Tool::Skill;
        let skill_message = Conversation::from_skills(&self.skills);
        self.conversation_history.0.push(skill_message);

        let mut query = json!({
            "messages": self.conversation_history.to_spec(),
            "tools": [
                read_tool.to_spec(),
                write_tool.to_spec(),
                bash_tool.to_spec(),
                skill_tool.to_spec()
            ],
            "model": "anthropic/claude-haiku-4.5",
        });

        for _ in 0..MAX_LOOP {
            eprintln!(
                "---- begining of the loop\n{}",
                serde_json::to_string_pretty(&query).unwrap()
            );
            let response: Value = self.client.chat().create_byot(query).await?;

            self.conversation_history
                .add_response(&response["choices"][0]["message"].to_string());
            dbg!(&self.conversation_history);
            if let Some(tool_calls) = response["choices"][0]["message"]["tool_calls"].as_array() {
                for tool_call in tool_calls {
                    let mut tool_call: ToolCall =
                        serde_json::from_value(tool_call.clone()).unwrap();
                    tool_call.function.active_skills = self.skills.clone();
                    if tool_call.function.name == Tool::Skill {
                        let c = self.client.config();
                        let c = (c as &dyn Any).downcast_ref::<OpenAIConfig>();
                        tool_call.function.agent_config = c.cloned()
                    }
                    dbg!(&tool_call);
                    let response = Conversation {
                        role: Role::Tool,
                        tool_call_id: Some(tool_call.id.clone()),
                        content: Some(tool_call.function.execute()),
                        tool_calls: None,
                    };
                    self.conversation_history.0.push(response);
                }
            } else {
                if let Some(content) = response["choices"][0]["message"]["content"].as_str() {
                    return Ok(String::from(content));
                }
                break;
            }
            query = json!({
                "messages": &self.conversation_history.to_spec(),
                "tools": [
                    read_tool.to_spec(),
                    write_tool.to_spec(),
                    bash_tool.to_spec(),
                    skill_tool.to_spec()
                ],
                "model": "anthropic/claude-haiku-4.5",
            });
        }
        unreachable!("loop should have ended before");
    }
}

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'p', long)]
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let base_url = env::var("OPENROUTER_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());

    let api_key = env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
        eprintln!("OPENROUTER_API_KEY is not set");
        process::exit(1);
    });

    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);

    let mut agent = Agent::default().with_config(config);
    agent.add_user_message(args.prompt);
    println!("{}", agent.run_agent_loop().await?);
    Ok(())
}
