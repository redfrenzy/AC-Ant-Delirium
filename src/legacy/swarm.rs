use crate::AuditResult;
use crate::config::AuditConfig;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
struct GovernanceTask {
    pub file_path: String,
    pub violation_count: usize,
    pub owner_id: Option<usize>,
}

#[derive(Clone, Default)]
struct AgentContext {
    pub internal_id: usize,
    pub worktree_path: String,
    pub status: String,
    pub staging_files: Vec<String>,
    pub current_task_intent: String,
    pub assigned_mission: Option<String>,
    pub last_ping: u64,
}

#[derive(Default, Clone)]
struct ConnectedState {
    pub total_active_agents: usize,
    pub agents: Vec<AgentContext>,
    pub tasks: Vec<GovernanceTask>,
}

/// [WHAT] Módulo orquestrador de Swarm dinâmico que detecta o ambiente git físico (Worktree vs Base Clone) e coordena a distribuição algorítmica de violações P0/P1 baseando-se no Censo Central `_connected.json`.
/// [WHY] Permite que N agentes de IA editem o mesmo repositório simultaneamente através de sandboxes físicos (Worktrees) sem gerar conflitos de merge ou sobreposição de contexto. Cada IA assume jurisdição de um subconjunto de problemas matematicamente isolados.
/// [WHERE] Camada de Estado Global (Swarm State Machine) ativada apenas se repository=false no manifesto do Geode.
pub fn orchestrate(total_result: &mut AuditResult, config: &AuditConfig) {
    if total_result.violations.is_empty() { return; }

    // DETECÇÃO DE MODO SWARM VIA PATH (ORCHESTRATOR VS AGENT)
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let canonical_cwd = fs::canonicalize(&current_dir).unwrap_or(current_dir);
    
    let is_orchestrator = match &config.hierarchy.repository_path {
        Some(repo_path) => {
            let repo_p = Path::new(repo_path);
            let canonical_repo = fs::canonicalize(repo_p).unwrap_or(repo_p.to_path_buf());
            canonical_cwd == canonical_repo
        },
        None => true, // Fallback para modo global se não houver path definido
    };

    if is_orchestrator {
        total_result.logs.push("[REPOSITORY MODE] Orquestrador Global ativo. Visão completa do projeto liberada. Agentes assíncronos desativados.".to_string());
        return; // Não filtra nada, retorna todos os erros pro shadow.json
    }

    // --- SWARM MODE ACTIVE ---
    
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let worktree_name = current_dir.to_string_lossy().to_string();

    // 1. Conectar ao Banco de Dados Tri-Manifesto
    let mut state = load_swarm_state();

    // 2. Registrar este Agente e atualizar Heartbeat
    let my_staging = get_git_status_files();
    let intent_env = std::env::var("SWARM_INTENT").unwrap_or_else(|_| "".to_string());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    
    let agent_id = register_or_update_agent(&mut state, &worktree_name, &my_staging, &intent_env, now);

    // 3. Atualizar Quadro de Missões (Sync de Violações)
    use std::collections::HashMap;
    let mut file_counts = HashMap::new();
    let current_dir_str = current_dir.to_string_lossy().to_string();

    for v in &mut total_result.violations {
        // Normalizar path: Remover o prefixo da worktree para que todos os agentes vejam o mesmo path lógico
        if v.file_path.starts_with(&current_dir_str) {
            let relative = v.file_path[current_dir_str.len()..].trim_start_matches('/').to_string();
            v.file_path = relative;
        }
        *file_counts.entry(v.file_path.clone()).or_insert(0) += 1;
    }
    
    // Atualizar contadores no governance.json para arquivos conhecidos da jurisdição deste agente
    for task in &mut state.tasks {
        if let Some(&count) = file_counts.get(&task.file_path) {
            task.violation_count = count;
        } else {
             // Se o arquivo não está no total_result da jurisdição (ex: src/*), mas nós o conhecemos...
             // Precisamos de cuidado aqui para não zerar arquivos que este agente não audita.
             // Como cada plugin agora é centralizado, o total_result da auditoria inicial é completo.
             task.violation_count = 0; 
        }
    }

    // Adicionar novos arquivos ignorados anteriormente
    for (fp, &count) in &file_counts {
        if !state.tasks.iter().any(|t| &t.file_path == fp) {
            state.tasks.push(GovernanceTask { file_path: fp.clone(), violation_count: count, owner_id: None });
        }
    }

    // 4. Lógica de Missão (Persistente)
    let mut assigned_file: Option<String> = None;
    
    // 4.1 Check se já tenho missão
    if let Some(agent) = state.agents.iter().find(|a| a.internal_id == agent_id) {
        if let Some(ref mission) = agent.assigned_mission {
             if let Some(task) = state.tasks.iter().find(|t| &t.file_path == mission) {
                 if task.violation_count > 0 {
                     assigned_file = Some(mission.clone());
                 }
             }
        }
    }

    // 4.2 Se não tenho missão válida, pegar uma nova
    if assigned_file.is_none() {
        state.tasks.sort_by(|a, b| b.violation_count.cmp(&a.violation_count));
        for task in &mut state.tasks {
            if task.violation_count > 0 && (task.owner_id.is_none() || task.owner_id == Some(agent_id)) {
                task.owner_id = Some(agent_id);
                assigned_file = Some(task.file_path.clone());
                if let Some(agent) = state.agents.iter_mut().find(|a| a.internal_id == agent_id) {
                    agent.assigned_mission = Some(task.file_path.clone());
                }
                break;
            }
        }
    }

    // 5. Salvar Estado Tri-Manifesto
    save_swarm_state(&state);

    // 6. Filtrar Visão do Agente
    if let Some(ref elected_fp) = assigned_file {
        total_result.violations.retain(|v| &v.file_path == elected_fp);
        let count = file_counts.get(elected_fp).unwrap_or(&0);
        total_result.logs.push(format!("[SWARM MISSION] Alvo Designado ({} anomalias): {}.", count, elected_fp));
    } else {
        total_result.violations.clear();
        total_result.logs.push("[SWARM STANDBY] Setor limpo! Nenhuma missão disponível no momento.".to_string());
    }
}


fn get_state_dir() -> PathBuf {
    let project_name = get_project_name();
    let mut p = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    p.pop(); // remove delirium_cli
    p.push(&project_name);
    let _ = fs::create_dir_all(&p);
    p
}

pub fn get_project_name() -> String {
    // 1. Tentar detectar via arquivo .git (Worktree)
    if let Ok(content) = fs::read_to_string(".git") {
        if content.starts_with("gitdir:") {
            // Exemplo: "gitdir: /home/user/project/.git/worktrees/name"
            if let Some(git_pos) = content.find("/.git/worktrees/") {
                let base_path_str = &content[7..git_pos];
                let base_path = Path::new(base_path_str);
                if let Some(name) = base_path.file_name() {
                    return name.to_string_lossy().to_string();
                }
            }
        }
    }

    // 2. Fallback para o nome do diretório atual
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(name) = cwd.file_name() {
            return name.to_string_lossy().to_string();
        }
    }

    "Global-Swarm".to_string()
}

fn load_swarm_state() -> ConnectedState {
    let mut state = ConnectedState::default();
    let dir = get_state_dir();
    if let Ok(content) = fs::read_to_string(dir.join("connected.json")) {
        // Split por '{' para pegar cada objeto de agente individualmente
        for block in content.split('{').skip(1) {
            let mut cur_agent = AgentContext::default();
            cur_agent.status = "ACTIVE".to_string();
            
            for line in block.split('\n') {
                let line = line.trim();
                if line.contains("\"internal_id\"") { cur_agent.internal_id = parse_json_usize(line); }
                else if line.contains("\"worktree_path\"") { cur_agent.worktree_path = parse_json_string(line); }
                else if line.contains("\"assigned_mission\"") { 
                    let m = parse_json_string(line);
                    cur_agent.assigned_mission = if m == "None" || m.is_empty() { None } else { Some(m) };
                }
                else if line.contains("\"last_ping\"") { cur_agent.last_ping = parse_json_u64(line); }
            }
            
            if !cur_agent.worktree_path.is_empty() {
                // Evita carregar duplicatas do arquivo sujo para a memória
                let folder = cur_agent.worktree_path.replace("\\", "/").trim_end_matches('/').split('/').last().unwrap_or(&cur_agent.worktree_path).to_string();
                if !state.agents.iter().any(|a| a.worktree_path.contains(&folder)) {
                    state.agents.push(cur_agent);
                }
            }
        }
    }

    // 2. CARREGAR TAREFAS (governance.json)
    if let Ok(content) = fs::read_to_string(dir.join("governance.json")) {
        let mut cur_task = GovernanceTask::default();
        for line in content.lines() {
            if line.contains('{') { cur_task = GovernanceTask::default(); }
            else if line.contains("\"file_path\"") { cur_task.file_path = parse_json_string(line); }
            else if line.contains("\"violation_count\"") { cur_task.violation_count = parse_json_usize(line); }
            else if line.contains("\"owner_id\"") { 
                let val_str = parse_json_raw_value(line);
                if val_str.contains("-1") {
                    cur_task.owner_id = None;
                } else {
                    cur_task.owner_id = Some(val_str.parse().unwrap_or(0));
                }
            }
            else if line.contains('}') {
                if !cur_task.file_path.is_empty() { state.tasks.push(cur_task.clone()); }
            }
        }
    }

    state.total_active_agents = state.agents.len();
    state
}

fn save_swarm_state(state: &ConnectedState) {
    let dir = get_state_dir();

    // 1. SALVAR connected.json
    let mut conn_json = String::from("{\n  \"agents\": [\n");
    for (i, a) in state.agents.iter().enumerate() {
        conn_json.push_str("    {\n");
        conn_json.push_str(&format!("      \"internal_id\": {},\n", a.internal_id));
        conn_json.push_str(&format!("      \"worktree_path\": \"{}\",\n", a.worktree_path.replace("\\", "/")));
        conn_json.push_str(&format!("      \"assigned_mission\": \"{}\",\n", a.assigned_mission.as_deref().unwrap_or("None")));
        conn_json.push_str(&format!("      \"last_ping\": {}\n", a.last_ping));
        conn_json.push_str("    }");
        if i < state.agents.len() - 1 { conn_json.push_str(","); }
        conn_json.push_str("\n");
    }
    conn_json.push_str("  ]\n}");
    let _ = fs::write(dir.join("connected.json"), conn_json);

    // 2. SALVAR governance.json
    let mut gov_json = String::from("{\n  \"tasks\": [\n");
    for (i, t) in state.tasks.iter().enumerate() {
        gov_json.push_str("    {\n");
        gov_json.push_str(&format!("      \"file_path\": \"{}\",\n", t.file_path));
        gov_json.push_str(&format!("      \"violation_count\": {},\n", t.violation_count));
        gov_json.push_str(&format!("      \"owner_id\": {}\n", t.owner_id.map(|id| id as i32).unwrap_or(-1)));
        gov_json.push_str("    }");
        if i < state.tasks.len() - 1 { gov_json.push_str(","); }
        gov_json.push_str("\n");
    }
    gov_json.push_str("  ]\n}");
    let _ = fs::write(dir.join("governance.json"), gov_json);
}

fn parse_json_string(line: &str) -> String {
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() > 1 {
        let val = parts[1].trim()
            .replace("\"", "")
            .replace(",", "")
            .replace("}", "")
            .replace("]", "")
            .trim()
            .to_string();
        return val;
    }
    String::new()
}

fn parse_json_usize(line: &str) -> usize {
    parse_json_raw_value(line).parse().unwrap_or(0)
}

fn parse_json_u64(line: &str) -> u64 {
    parse_json_raw_value(line).parse().unwrap_or(0)
}

fn parse_json_raw_value(line: &str) -> String {
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() > 1 {
        let val = parts[1].trim()
            .trim_matches(',')
            .trim_matches('}')
            .trim_matches(']')
            .trim_matches('"')
            .trim();
        return val.to_string();
    }
    String::new()
}

fn register_or_update_agent(state: &mut ConnectedState, worktree: &str, staging: &Vec<String>, intent: &str, now: u64) -> usize {
    let folder_id = worktree.replace("\\", "/").trim_end_matches('/').split('/').last().unwrap_or(worktree).to_string().to_lowercase();
    
    // 1. Procurar agente existente (Comparação Blindada)
    for agent in &mut state.agents {
        let saved_path = agent.worktree_path.replace("\\", "/").to_lowercase();
        if saved_path.ends_with(&folder_id) || folder_id.ends_with(&saved_path) {
            agent.staging_files = staging.clone();
            agent.last_ping = now;
            if !intent.is_empty() {
                agent.current_task_intent = intent.to_string();
            }
            return agent.internal_id;
        }
    }

    // 2. Criar novo ID Sequencial (Limpo)
    let new_id = state.agents.len();
    state.agents.push(AgentContext {
        internal_id: new_id,
        worktree_path: worktree.to_string(),
        status: "ACTIVE".to_string(),
        staging_files: staging.clone(),
        current_task_intent: if intent.is_empty() { "Uninitialized".to_string() } else { intent.to_string() },
        assigned_mission: None,
        last_ping: now,
    });
    state.total_active_agents = state.agents.len();
    new_id
}

/// Dispara furtivamente o git e extrai os caminhos dos arquivos modificados/staged.
fn get_git_status_files() -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(output) = std::process::Command::new("git")
        .args(&["status", "--porcelain"])
        .output()
    {
        let out_str = String::from_utf8_lossy(&output.stdout);
        for line in out_str.lines() {
            if line.len() > 3 {
                let file_path = line[3..].trim().to_string();
                if !file_path.is_empty() {
                    files.push(file_path);
                }
            }
        }
    }
    files
}

fn get_recent_master_commits() -> Vec<String> {
    let mut commits = Vec::new();
    // Use fallback to 'master' or 'HEAD' se não houver origin/main. Faremos um rev-list básico dos ultimos 3
    if let Ok(output) = std::process::Command::new("git")
        .args(&["log", "-n", "3", "--oneline"])
        .output()
    {
         let out_str = String::from_utf8_lossy(&output.stdout);
         for line in out_str.lines() {
             if !line.is_empty() {
                 commits.push(line.to_string());
             }
         }
    }
    commits
}
