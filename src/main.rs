use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::fs::read_to_string;
use std::{env, process};

const MAX_LOOP: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Tool {
    Read,
    Write,
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
    //arguments: HashMap<String, String>,
    arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

impl FunctionCall {
    /// Execute a tool call
    fn execute(&self) -> String {
        match &self.name {
            Tool::Read => self.read(),
            Tool::Write => self.write(),
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    #[default]
    User,
    Assistant,
    Tool,
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

    let client = Client::with_config(config);
    let mut conversation_history = ConversationHistory::default();

    #[allow(unused_variables)]
    let read_tool = Tool::Read;
    let write_tool = Tool::Write;
    let init_message = Conversation {
        role: Role::User,
        content: args.prompt.into(),
        ..Default::default()
    };
    conversation_history.0.push(init_message);
    let mut query = json!({
        "messages": conversation_history.to_spec(),
        "tools": [read_tool.to_spec(), write_tool.to_spec()],
        "model": "anthropic/claude-haiku-4.5",
    });

    for _ in 0..MAX_LOOP {
        eprintln!(
            "---- begining of the loop\n{}",
            serde_json::to_string_pretty(&query).unwrap()
        );
        let response: Value = client.chat().create_byot(query).await?;

        conversation_history.add_response(&response["choices"][0]["message"].to_string());
        dbg!(&conversation_history);
        if let Some(tool_calls) = response["choices"][0]["message"]["tool_calls"].as_array() {
            for tool_call in tool_calls {
                let tool_call: ToolCall = serde_json::from_value(tool_call.clone()).unwrap();
                dbg!(&tool_call);
                let response = Conversation {
                    role: Role::Tool,
                    tool_call_id: Some(tool_call.id.clone()),
                    content: Some(tool_call.function.execute()),
                    tool_calls: None,
                };
                conversation_history.0.push(response);
            }
        } else {
            if let Some(content) = response["choices"][0]["message"]["content"].as_str() {
                println!("{}", content);
            }
            break;
        }
        query = json!({
            "messages": conversation_history.to_spec(),
            "tools": [read_tool.to_spec(), write_tool.to_spec()],
            "model": "anthropic/claude-haiku-4.5",
        });
    }

    Ok(())
}
