use std::collections::HashSet;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use fsrs::{MemoryState, FSRS};
use serde::{Deserialize, Serialize};
use sqlx::{Acquire, Row};
use tauri::State;
use url::Url;
use uuid::Uuid;

use crate::product::AppState;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningGoalItem {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub current_level: String,
    pub target_level: String,
    pub target_date: Option<String>,
    pub weekly_minutes: i64,
    pub daily_minutes: i64,
    pub language: String,
    pub resource_preferences: Vec<String>,
    pub budget_mode: String,
    pub auto_add_lessons: bool,
    pub status: String,
    pub node_count: i64,
    pub due_count: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeNodeItem {
    pub id: String,
    pub goal_id: String,
    pub name: String,
    pub plain_explanation: String,
    pub stage_outcome: String,
    pub estimated_minutes: Option<i64>,
    pub prerequisite_ids: Vec<String>,
    pub status: String,
    pub mastery_score: i64,
    pub quiz_count: i64,
    pub due_count: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningResourceItem {
    pub id: String,
    pub node_id: String,
    pub title: String,
    pub author: Option<String>,
    pub published_date: Option<String>,
    pub resource_type: String,
    pub duration_minutes: Option<i64>,
    pub difficulty: Option<String>,
    pub cost: Option<String>,
    pub version_fit: Option<String>,
    pub url: String,
    pub recommendation_reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuizItemView {
    pub id: String,
    pub node_id: String,
    pub node_name: String,
    pub question_type: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub explanation_after_submit: Option<String>,
    pub difficulty: i64,
    pub due_utc: String,
    pub is_due: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuizResult {
    pub attempt_id: String,
    pub is_correct: bool,
    pub correct_answer: Vec<String>,
    pub explanation: String,
    pub next_due_utc: String,
    pub rating: u32,
    pub mastery_score: i64,
    pub node_status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLearningGoalInput {
    pub name: String,
    pub purpose: String,
    pub current_level: String,
    pub target_level: String,
    pub target_date: Option<String>,
    pub weekly_minutes: Option<i64>,
    pub daily_minutes: Option<i64>,
    pub language: Option<String>,
    #[serde(default)]
    pub resource_preferences: Vec<String>,
    pub budget_mode: Option<String>,
    #[serde(default)]
    pub auto_add_lessons: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateKnowledgeNodeInput {
    pub goal_id: String,
    pub name: String,
    pub plain_explanation: String,
    pub stage_outcome: String,
    pub estimated_minutes: Option<i64>,
    #[serde(default)]
    pub prerequisite_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResourceInput {
    pub node_id: String,
    pub title: String,
    pub author: Option<String>,
    pub published_date: Option<String>,
    pub resource_type: String,
    pub duration_minutes: Option<i64>,
    pub difficulty: Option<String>,
    pub cost: Option<String>,
    pub version_fit: Option<String>,
    pub url: String,
    pub recommendation_reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuizInput {
    pub node_id: String,
    pub question_type: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_answer: Vec<String>,
    pub explanation: String,
    pub difficulty: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitQuizInput {
    pub quiz_id: String,
    pub answer: Vec<String>,
    pub confidence: String,
}

#[tauri::command]
pub async fn list_learning_goals(
    state: State<'_, AppState>,
) -> Result<Vec<LearningGoalItem>, String> {
    let rows = sqlx::query(
        "SELECT g.id, g.name, g.purpose, g.current_level, g.target_level,
                g.target_date, g.weekly_minutes, g.daily_minutes, g.language,
                g.resource_preferences_json, g.budget_mode, g.auto_add_lessons, g.status,
                COUNT(DISTINCT n.id) AS node_count,
                COUNT(DISTINCT CASE WHEN c.due_utc <= ? THEN c.id END) AS due_count
         FROM learning_goals g
         LEFT JOIN knowledge_nodes n ON n.goal_id = g.id AND n.deleted_at_utc IS NULL
         LEFT JOIN quiz_items q ON q.node_id = n.id
         LEFT JOIN fsrs_cards c ON c.quiz_item_id = q.id
         WHERE g.deleted_at_utc IS NULL
         GROUP BY g.id ORDER BY g.created_at_utc",
    )
    .bind(utc_text(Utc::now()))
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(row_to_goal).collect()
}

#[tauri::command]
pub async fn create_learning_goal(
    state: State<'_, AppState>,
    input: CreateLearningGoalInput,
) -> Result<LearningGoalItem, String> {
    let name = clean_required(&input.name, 160, "学习目标")?;
    let purpose = clean_optional(&input.purpose, 1200, "学习用途")?;
    let current_level = clean_level(&input.current_level)?;
    let target_level = clean_level(&input.target_level)?;
    let weekly_minutes = input.weekly_minutes.unwrap_or(180);
    let daily_minutes = input.daily_minutes.unwrap_or(30);
    let language = clean_required(input.language.as_deref().unwrap_or("zh-CN"), 30, "学习语言")?;
    let resource_preferences = input
        .resource_preferences
        .into_iter()
        .map(|value| clean_required(&value, 60, "资源偏好"))
        .collect::<Result<Vec<_>, _>>()?;
    if resource_preferences.len() > 20 {
        return Err("资源偏好最多 20 项".to_owned());
    }
    let budget_mode = input.budget_mode.unwrap_or_else(|| "free_first".to_owned());
    if !matches!(budget_mode.as_str(), "free_first" | "paid_allowed") {
        return Err("学习资源预算模式无效".to_owned());
    }
    if !(10..=10_080).contains(&weekly_minutes) || !(5..=1_440).contains(&daily_minutes) {
        return Err("学习时间预算超出允许范围".to_owned());
    }
    if let Some(date) = input.target_date.as_deref() {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| "目标日期格式无效".to_owned())?;
    }
    let id = Uuid::now_v7();
    let now = utc_text(Utc::now());
    sqlx::query(
        "INSERT INTO learning_goals(
            id, name, purpose, current_level, target_level, target_date,
            weekly_minutes, daily_minutes, language, resource_preferences_json,
            budget_mode, auto_add_lessons, created_at_utc, updated_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(purpose)
    .bind(current_level)
    .bind(target_level)
    .bind(input.target_date)
    .bind(weekly_minutes)
    .bind(daily_minutes)
    .bind(language)
    .bind(serde_json::to_string(&resource_preferences).map_err(json_error)?)
    .bind(budget_mode)
    .bind(i64::from(input.auto_add_lessons))
    .bind(&now)
    .bind(&now)
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?;
    fetch_goal(&state.store.pool, id).await
}

#[tauri::command]
pub async fn list_knowledge_nodes(
    state: State<'_, AppState>,
    goal_id: String,
) -> Result<Vec<KnowledgeNodeItem>, String> {
    let goal_id = parse_uuid(&goal_id, "学习目标标识")?;
    let rows = sqlx::query(
        "SELECT n.id, n.goal_id, n.name, n.plain_explanation, n.stage_outcome,
                n.estimated_minutes, n.status, n.mastery_score,
                COALESCE((SELECT json_group_array(e.prerequisite_node_id)
                          FROM knowledge_edges e WHERE e.dependent_node_id = n.id), '[]')
                    AS prerequisite_ids_json,
                COUNT(DISTINCT q.id) AS quiz_count,
                COUNT(DISTINCT CASE WHEN c.due_utc <= ? THEN c.id END) AS due_count
         FROM knowledge_nodes n
         LEFT JOIN quiz_items q ON q.node_id = n.id
         LEFT JOIN fsrs_cards c ON c.quiz_item_id = q.id
         WHERE n.goal_id = ? AND n.deleted_at_utc IS NULL
         GROUP BY n.id ORDER BY n.sort_order, n.created_at_utc",
    )
    .bind(utc_text(Utc::now()))
    .bind(goal_id.to_string())
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(row_to_node).collect()
}

#[tauri::command]
pub async fn create_knowledge_node(
    state: State<'_, AppState>,
    input: CreateKnowledgeNodeInput,
) -> Result<KnowledgeNodeItem, String> {
    let goal_id = parse_uuid(&input.goal_id, "学习目标标识")?;
    let name = clean_required(&input.name, 160, "知识点")?;
    let explanation = clean_optional(&input.plain_explanation, 3000, "通俗解释")?;
    let stage_outcome = clean_required(&input.stage_outcome, 500, "阶段成果")?;
    if input
        .estimated_minutes
        .is_some_and(|value| !(1..=10_080).contains(&value))
    {
        return Err("预计学习时间超出允许范围".to_owned());
    }
    if input.prerequisite_ids.len() > 50 {
        return Err("单个知识点最多允许 50 个前置知识".to_owned());
    }
    let prerequisite_ids = input
        .prerequisite_ids
        .iter()
        .map(|value| parse_uuid(value, "前置知识标识"))
        .collect::<Result<Vec<_>, _>>()?;
    let unique_prerequisites = prerequisite_ids.iter().copied().collect::<HashSet<_>>();
    if unique_prerequisites.len() != prerequisite_ids.len() {
        return Err("前置知识不能重复".to_owned());
    }
    let sort_order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM knowledge_nodes WHERE goal_id = ?",
    )
    .bind(goal_id.to_string())
    .fetch_one(&state.store.pool)
    .await
    .map_err(db_error)?;
    let id = Uuid::now_v7();
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    for prerequisite_id in &prerequisite_ids {
        let belongs_to_goal: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM knowledge_nodes
             WHERE id = ? AND goal_id = ? AND deleted_at_utc IS NULL",
        )
        .bind(prerequisite_id.to_string())
        .bind(goal_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
        if belongs_to_goal != 1 {
            return Err("前置知识必须来自同一个有效学习目标".to_owned());
        }
    }
    sqlx::query(
        "INSERT INTO knowledge_nodes(
            id, goal_id, name, plain_explanation, stage_outcome, estimated_minutes,
            sort_order, created_at_utc, updated_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(goal_id.to_string())
    .bind(name)
    .bind(explanation)
    .bind(stage_outcome)
    .bind(input.estimated_minutes)
    .bind(sort_order)
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    for prerequisite_id in prerequisite_ids {
        sqlx::query(
            "INSERT INTO knowledge_edges(prerequisite_node_id, dependent_node_id, created_at_utc)
             VALUES (?, ?, ?)",
        )
        .bind(prerequisite_id.to_string())
        .bind(id.to_string())
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    }
    transaction.commit().await.map_err(db_error)?;
    fetch_node(&state.store.pool, id).await
}

#[tauri::command]
pub async fn list_learning_resources(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Vec<LearningResourceItem>, String> {
    let node_id = parse_uuid(&node_id, "知识点标识")?;
    let rows = sqlx::query(
        "SELECT id, node_id, title, author, published_date, resource_type,
                duration_minutes, difficulty, cost, version_fit, url, recommendation_reason
         FROM learning_resources WHERE node_id = ? ORDER BY created_at_utc",
    )
    .bind(node_id.to_string())
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(row_to_resource).collect()
}

#[tauri::command]
pub async fn create_learning_resource(
    state: State<'_, AppState>,
    input: CreateResourceInput,
) -> Result<LearningResourceItem, String> {
    let node_id = parse_uuid(&input.node_id, "知识点标识")?;
    let title = clean_required(&input.title, 240, "资源标题")?;
    let url = validate_resource_url(&input.url)?.to_string();
    if input
        .duration_minutes
        .is_some_and(|value| !(1..=10_080).contains(&value))
    {
        return Err("资源时长超出允许范围".to_owned());
    }
    let id = Uuid::now_v7();
    let now = utc_text(Utc::now());
    sqlx::query(
        "INSERT INTO learning_resources(
            id, node_id, title, author, published_date, resource_type, language,
            duration_minutes, difficulty, cost, version_fit, stale_risk, url,
            recommendation_reason, created_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, 'zh-CN', ?, ?, ?, ?, 'unknown', ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(node_id.to_string())
    .bind(title)
    .bind(clean_optional_option(input.author, 160, "作者")?)
    .bind(input.published_date)
    .bind(clean_required(&input.resource_type, 60, "资源类型")?)
    .bind(input.duration_minutes)
    .bind(clean_optional_option(input.difficulty, 60, "难度")?)
    .bind(clean_optional_option(input.cost, 60, "费用")?)
    .bind(clean_optional_option(input.version_fit, 120, "版本适配")?)
    .bind(url)
    .bind(clean_optional(
        &input.recommendation_reason,
        1000,
        "推荐理由",
    )?)
    .bind(now)
    .execute(&state.store.pool)
    .await
    .map_err(db_error)?;
    fetch_resource(&state.store.pool, id).await
}

#[tauri::command]
pub async fn create_quiz_item(
    state: State<'_, AppState>,
    input: CreateQuizInput,
) -> Result<QuizItemView, String> {
    let node_id = parse_uuid(&input.node_id, "知识点标识")?;
    if !matches!(
        input.question_type.as_str(),
        "single" | "multiple" | "true_false"
    ) {
        return Err("首版本地判分支持单选、多选和判断题".to_owned());
    }
    let prompt = clean_required(&input.prompt, 2000, "题目")?;
    let options = input
        .options
        .into_iter()
        .map(|value| clean_required(&value, 500, "选项"))
        .collect::<Result<Vec<_>, _>>()?;
    let answers = input
        .correct_answer
        .into_iter()
        .map(|value| clean_required(&value, 500, "正确答案"))
        .collect::<Result<Vec<_>, _>>()?;
    if options.len() < 2 || options.len() > 8 || answers.is_empty() {
        return Err("客观题需要 2–8 个选项和至少一个正确答案".to_owned());
    }
    if answers.iter().any(|answer| !options.contains(answer)) {
        return Err("正确答案必须来自给定选项".to_owned());
    }
    if input.question_type != "multiple" && answers.len() != 1 {
        return Err("单选或判断题只能有一个正确答案".to_owned());
    }
    let difficulty = input.difficulty.unwrap_or(1);
    if !(1..=5).contains(&difficulty) {
        return Err("题目难度必须为 1 到 5".to_owned());
    }
    let quiz_id = Uuid::now_v7();
    let card_id = Uuid::now_v7();
    let now = utc_text(Utc::now());
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    sqlx::query(
        "INSERT INTO quiz_items(
            id, node_id, question_type, prompt, options_json, answer_json,
            explanation, difficulty, created_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(quiz_id.to_string())
    .bind(node_id.to_string())
    .bind(input.question_type)
    .bind(prompt)
    .bind(serde_json::to_string(&options).map_err(json_error)?)
    .bind(serde_json::to_string(&answers).map_err(json_error)?)
    .bind(clean_optional(&input.explanation, 3000, "答案解释")?)
    .bind(difficulty)
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO fsrs_cards(
            id, quiz_item_id, due_utc, state, algorithm_version, params_version, updated_at_utc
         ) VALUES (?, ?, ?, 'new', 'fsrs-rs-6.6.2', 'default-v1', ?)",
    )
    .bind(card_id.to_string())
    .bind(quiz_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "UPDATE knowledge_nodes SET status = 'learning', updated_at_utc = ?,
            version = version + 1 WHERE id = ? AND status = 'not_started'",
    )
    .bind(&now)
    .bind(node_id.to_string())
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    fetch_quiz(&state.store.pool, quiz_id, true).await
}

#[tauri::command]
pub async fn list_quiz_items(
    state: State<'_, AppState>,
    goal_id: Option<String>,
    due_only: bool,
) -> Result<Vec<QuizItemView>, String> {
    let goal_id = goal_id
        .as_deref()
        .map(|value| parse_uuid(value, "学习目标标识"))
        .transpose()?;
    let now = utc_text(Utc::now());
    let rows = sqlx::query(
        "SELECT q.id, q.node_id, n.name AS node_name, q.question_type, q.prompt,
                q.options_json, q.difficulty, c.due_utc
         FROM quiz_items q
         JOIN knowledge_nodes n ON n.id = q.node_id
         JOIN fsrs_cards c ON c.quiz_item_id = q.id
         JOIN learning_goals g ON g.id = n.goal_id
         WHERE q.id IS NOT NULL AND n.deleted_at_utc IS NULL AND g.deleted_at_utc IS NULL
           AND (? IS NULL OR g.id = ?)
           AND (? = 0 OR c.due_utc <= ?)
         ORDER BY c.due_utc, n.sort_order LIMIT 100",
    )
    .bind(goal_id.map(|value| value.to_string()))
    .bind(goal_id.map(|value| value.to_string()))
    .bind(i64::from(due_only))
    .bind(&now)
    .fetch_all(&state.store.pool)
    .await
    .map_err(db_error)?;
    rows.iter().map(|row| row_to_quiz(row, &now)).collect()
}

#[tauri::command]
pub async fn submit_quiz(
    state: State<'_, AppState>,
    input: SubmitQuizInput,
) -> Result<QuizResult, String> {
    let quiz_id = parse_uuid(&input.quiz_id, "题目标识")?;
    if !matches!(input.confidence.as_str(), "sure" | "unsure" | "skipped") {
        return Err("答题信心选项无效".to_owned());
    }
    let answer = input
        .answer
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let row = sqlx::query(
        "SELECT q.node_id, q.answer_json, q.explanation, c.id AS card_id,
                c.due_utc, c.stability, c.difficulty, c.reps, c.lapses,
                c.last_review_utc
         FROM quiz_items q JOIN fsrs_cards c ON c.quiz_item_id = q.id
         WHERE q.id = ?",
    )
    .bind(quiz_id.to_string())
    .fetch_optional(&state.store.pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "题目不存在".to_owned())?;
    let correct_raw: String = row.try_get("answer_json").map_err(db_error)?;
    let correct_answer: Vec<String> = serde_json::from_str(&correct_raw).map_err(json_error)?;
    let is_correct = normalized_answers(&answer) == normalized_answers(&correct_answer);
    let rating = match (is_correct, input.confidence.as_str()) {
        (false, _) | (_, "skipped") => 1,
        (true, "unsure") => 2,
        (true, "sure") => 3,
        _ => 1,
    };
    let stability: f64 = row.try_get("stability").map_err(db_error)?;
    let difficulty: f64 = row.try_get("difficulty").map_err(db_error)?;
    let last_review: Option<String> = row.try_get("last_review_utc").map_err(db_error)?;
    let now_value = Utc::now();
    let elapsed_days = last_review
        .as_deref()
        .and_then(parse_utc)
        .map(|last| (now_value - last).num_days().max(0) as u32)
        .unwrap_or(0);
    let previous_state = (stability > 0.0 && difficulty > 0.0).then_some(MemoryState {
        stability: stability as f32,
        difficulty: difficulty as f32,
    });
    let next_states = FSRS::default()
        .next_states(previous_state, 0.9, elapsed_days)
        .map_err(|error| format!("FSRS 排程失败：{error}"))?;
    let next = match rating {
        1 => next_states.again,
        2 => next_states.hard,
        3 => next_states.good,
        _ => next_states.easy,
    };
    let interval_days = next.interval.round().max(1.0) as i64;
    let next_due = now_value + Duration::days(interval_days);
    let now = utc_text(now_value);
    let next_due_text = utc_text(next_due);
    let attempt_id = Uuid::now_v7();
    let node_id: String = row.try_get("node_id").map_err(db_error)?;
    let card_id: String = row.try_get("card_id").map_err(db_error)?;
    let previous_due: String = row.try_get("due_utc").map_err(db_error)?;
    let reps: i64 = row.try_get("reps").map_err(db_error)?;
    let lapses: i64 = row.try_get("lapses").map_err(db_error)?;
    let mut connection = state.store.pool.acquire().await.map_err(db_error)?;
    let mut transaction = connection.begin().await.map_err(db_error)?;
    sqlx::query(
        "INSERT INTO quiz_attempts(
            id, quiz_item_id, answer_json, is_correct, confidence, attempted_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(attempt_id.to_string())
    .bind(quiz_id.to_string())
    .bind(serde_json::to_string(&answer).map_err(json_error)?)
    .bind(i64::from(is_correct))
    .bind(&input.confidence)
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    if is_correct {
        sqlx::query("UPDATE mistake_book SET resolved_at_utc = ?, updated_at_utc = ? WHERE quiz_item_id = ?")
            .bind(&now)
            .bind(&now)
            .bind(quiz_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
    } else {
        sqlx::query(
            "INSERT INTO mistake_book(
                id, quiz_item_id, latest_attempt_id, created_at_utc, updated_at_utc
             ) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(quiz_item_id) DO UPDATE SET
                latest_attempt_id = excluded.latest_attempt_id,
                resolved_at_utc = NULL,
                updated_at_utc = excluded.updated_at_utc",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(quiz_id.to_string())
        .bind(attempt_id.to_string())
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    }
    sqlx::query(
        "UPDATE fsrs_cards SET due_utc = ?, stability = ?, difficulty = ?,
            state = 'review', reps = ?, lapses = ?, last_review_utc = ?, updated_at_utc = ?
         WHERE id = ?",
    )
    .bind(&next_due_text)
    .bind(f64::from(next.memory.stability))
    .bind(f64::from(next.memory.difficulty))
    .bind(reps + 1)
    .bind(lapses + i64::from(!is_correct))
    .bind(&now)
    .bind(&now)
    .bind(&card_id)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO review_logs(id, card_id, rating, previous_due_utc, next_due_utc, reviewed_at_utc)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&card_id)
    .bind(i64::from(rating))
    .bind(previous_due)
    .bind(&next_due_text)
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    let delta = if is_correct { 10 } else { -8 };
    sqlx::query(
        "INSERT INTO mastery_evidence(
            id, node_id, evidence_type, score_delta, source_id, created_at_utc
         ) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&node_id)
    .bind(if is_correct {
        "objective_quiz_correct"
    } else {
        "objective_quiz_incorrect"
    })
    .bind(delta)
    .bind(attempt_id.to_string())
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    let current_mastery: i64 =
        sqlx::query_scalar("SELECT mastery_score FROM knowledge_nodes WHERE id = ?")
            .bind(&node_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(db_error)?;
    let correct_attempts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM quiz_attempts a
         JOIN quiz_items q ON q.id = a.quiz_item_id
         WHERE q.node_id = ? AND a.is_correct = 1",
    )
    .bind(&node_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(db_error)?;
    let delayed_reviews: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM quiz_attempts later
         JOIN quiz_items q ON q.id = later.quiz_item_id
         WHERE q.node_id = ? AND EXISTS (
            SELECT 1 FROM quiz_attempts earlier
            JOIN quiz_items eq ON eq.id = earlier.quiz_item_id
            WHERE eq.node_id = q.node_id
              AND julianday(later.attempted_at_utc) - julianday(earlier.attempted_at_utc) >= 1
         )",
    )
    .bind(&node_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(db_error)?;
    let mut mastery = (current_mastery + delta).clamp(0, 100);
    if correct_attempts < 2 || delayed_reviews < 1 {
        mastery = mastery.min(79);
    }
    let node_status = if mastery >= 80 && correct_attempts >= 2 && delayed_reviews >= 1 {
        "mastered"
    } else if mastery >= 50 {
        "basic"
    } else {
        "learning"
    };
    sqlx::query(
        "UPDATE knowledge_nodes SET mastery_score = ?, status = ?,
            updated_at_utc = ?, version = version + 1 WHERE id = ?",
    )
    .bind(mastery)
    .bind(node_status)
    .bind(&now)
    .bind(&node_id)
    .execute(&mut *transaction)
    .await
    .map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    Ok(QuizResult {
        attempt_id: attempt_id.to_string(),
        is_correct,
        correct_answer,
        explanation: row.try_get("explanation").map_err(db_error)?,
        next_due_utc: next_due_text,
        rating,
        mastery_score: mastery,
        node_status: node_status.to_owned(),
    })
}

async fn fetch_goal(pool: &sqlx::SqlitePool, id: Uuid) -> Result<LearningGoalItem, String> {
    let row = sqlx::query(
        "SELECT g.id, g.name, g.purpose, g.current_level, g.target_level,
                g.target_date, g.weekly_minutes, g.daily_minutes, g.language,
                g.resource_preferences_json, g.budget_mode, g.auto_add_lessons, g.status,
                COUNT(DISTINCT n.id) AS node_count,
                COUNT(DISTINCT CASE WHEN c.due_utc <= ? THEN c.id END) AS due_count
         FROM learning_goals g
         LEFT JOIN knowledge_nodes n ON n.goal_id = g.id AND n.deleted_at_utc IS NULL
         LEFT JOIN quiz_items q ON q.node_id = n.id
         LEFT JOIN fsrs_cards c ON c.quiz_item_id = q.id
         WHERE g.id = ? AND g.deleted_at_utc IS NULL GROUP BY g.id",
    )
    .bind(utc_text(Utc::now()))
    .bind(id.to_string())
    .fetch_one(pool)
    .await
    .map_err(db_error)?;
    row_to_goal(&row)
}

async fn fetch_node(pool: &sqlx::SqlitePool, id: Uuid) -> Result<KnowledgeNodeItem, String> {
    let row = sqlx::query(
        "SELECT n.id, n.goal_id, n.name, n.plain_explanation, n.stage_outcome,
                n.estimated_minutes, n.status, n.mastery_score,
                COALESCE((SELECT json_group_array(e.prerequisite_node_id)
                          FROM knowledge_edges e WHERE e.dependent_node_id = n.id), '[]')
                    AS prerequisite_ids_json,
                COUNT(DISTINCT q.id) AS quiz_count,
                COUNT(DISTINCT CASE WHEN c.due_utc <= ? THEN c.id END) AS due_count
         FROM knowledge_nodes n
         LEFT JOIN quiz_items q ON q.node_id = n.id
         LEFT JOIN fsrs_cards c ON c.quiz_item_id = q.id
         WHERE n.id = ? AND n.deleted_at_utc IS NULL GROUP BY n.id",
    )
    .bind(utc_text(Utc::now()))
    .bind(id.to_string())
    .fetch_one(pool)
    .await
    .map_err(db_error)?;
    row_to_node(&row)
}

async fn fetch_resource(pool: &sqlx::SqlitePool, id: Uuid) -> Result<LearningResourceItem, String> {
    let row = sqlx::query(
        "SELECT id, node_id, title, author, published_date, resource_type,
                duration_minutes, difficulty, cost, version_fit, url, recommendation_reason
         FROM learning_resources WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_one(pool)
    .await
    .map_err(db_error)?;
    row_to_resource(&row)
}

async fn fetch_quiz(
    pool: &sqlx::SqlitePool,
    id: Uuid,
    _fresh: bool,
) -> Result<QuizItemView, String> {
    let now = utc_text(Utc::now());
    let row = sqlx::query(
        "SELECT q.id, q.node_id, n.name AS node_name, q.question_type, q.prompt,
                q.options_json, q.difficulty, c.due_utc
         FROM quiz_items q JOIN knowledge_nodes n ON n.id = q.node_id
         JOIN fsrs_cards c ON c.quiz_item_id = q.id WHERE q.id = ?",
    )
    .bind(id.to_string())
    .fetch_one(pool)
    .await
    .map_err(db_error)?;
    row_to_quiz(&row, &now)
}

fn row_to_goal(row: &sqlx::sqlite::SqliteRow) -> Result<LearningGoalItem, String> {
    Ok(LearningGoalItem {
        id: row.try_get("id").map_err(db_error)?,
        name: row.try_get("name").map_err(db_error)?,
        purpose: row.try_get("purpose").map_err(db_error)?,
        current_level: row.try_get("current_level").map_err(db_error)?,
        target_level: row.try_get("target_level").map_err(db_error)?,
        target_date: row.try_get("target_date").map_err(db_error)?,
        weekly_minutes: row.try_get("weekly_minutes").map_err(db_error)?,
        daily_minutes: row.try_get("daily_minutes").map_err(db_error)?,
        language: row.try_get("language").map_err(db_error)?,
        resource_preferences: serde_json::from_str(
            &row.try_get::<String, _>("resource_preferences_json")
                .map_err(db_error)?,
        )
        .map_err(json_error)?,
        budget_mode: row.try_get("budget_mode").map_err(db_error)?,
        auto_add_lessons: row
            .try_get::<i64, _>("auto_add_lessons")
            .map_err(db_error)?
            != 0,
        status: row.try_get("status").map_err(db_error)?,
        node_count: row.try_get("node_count").map_err(db_error)?,
        due_count: row.try_get("due_count").map_err(db_error)?,
    })
}

fn row_to_node(row: &sqlx::sqlite::SqliteRow) -> Result<KnowledgeNodeItem, String> {
    let prerequisite_ids: String = row.try_get("prerequisite_ids_json").map_err(db_error)?;
    Ok(KnowledgeNodeItem {
        id: row.try_get("id").map_err(db_error)?,
        goal_id: row.try_get("goal_id").map_err(db_error)?,
        name: row.try_get("name").map_err(db_error)?,
        plain_explanation: row.try_get("plain_explanation").map_err(db_error)?,
        stage_outcome: row.try_get("stage_outcome").map_err(db_error)?,
        estimated_minutes: row.try_get("estimated_minutes").map_err(db_error)?,
        prerequisite_ids: serde_json::from_str(&prerequisite_ids).map_err(json_error)?,
        status: row.try_get("status").map_err(db_error)?,
        mastery_score: row.try_get("mastery_score").map_err(db_error)?,
        quiz_count: row.try_get("quiz_count").map_err(db_error)?,
        due_count: row.try_get("due_count").map_err(db_error)?,
    })
}

fn row_to_resource(row: &sqlx::sqlite::SqliteRow) -> Result<LearningResourceItem, String> {
    Ok(LearningResourceItem {
        id: row.try_get("id").map_err(db_error)?,
        node_id: row.try_get("node_id").map_err(db_error)?,
        title: row.try_get("title").map_err(db_error)?,
        author: row.try_get("author").map_err(db_error)?,
        published_date: row.try_get("published_date").map_err(db_error)?,
        resource_type: row.try_get("resource_type").map_err(db_error)?,
        duration_minutes: row.try_get("duration_minutes").map_err(db_error)?,
        difficulty: row.try_get("difficulty").map_err(db_error)?,
        cost: row.try_get("cost").map_err(db_error)?,
        version_fit: row.try_get("version_fit").map_err(db_error)?,
        url: row.try_get("url").map_err(db_error)?,
        recommendation_reason: row.try_get("recommendation_reason").map_err(db_error)?,
    })
}

fn row_to_quiz(row: &sqlx::sqlite::SqliteRow, now: &str) -> Result<QuizItemView, String> {
    let options_raw: String = row.try_get("options_json").map_err(db_error)?;
    let due_utc: String = row.try_get("due_utc").map_err(db_error)?;
    Ok(QuizItemView {
        id: row.try_get("id").map_err(db_error)?,
        node_id: row.try_get("node_id").map_err(db_error)?,
        node_name: row.try_get("node_name").map_err(db_error)?,
        question_type: row.try_get("question_type").map_err(db_error)?,
        prompt: row.try_get("prompt").map_err(db_error)?,
        options: serde_json::from_str(&options_raw).map_err(json_error)?,
        explanation_after_submit: None,
        difficulty: row.try_get("difficulty").map_err(db_error)?,
        is_due: due_utc.as_str() <= now,
        due_utc,
    })
}

fn normalized_answers(values: &[String]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| value.trim().to_lowercase())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn validate_resource_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|_| "资源链接无效".to_owned())?;
    if url.scheme() != "https" || url.host_str().is_none() || url.username() != "" {
        return Err("资源链接必须是公开 HTTPS 地址".to_owned());
    }
    Ok(url)
}

fn clean_level(value: &str) -> Result<String, String> {
    let value = clean_required(value, 80, "学习水平")?;
    Ok(value)
}

fn clean_required(value: &str, max: usize, label: &str) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().count() > max {
        return Err(format!("{label}不能为空且不能超过 {max} 个字符"));
    }
    Ok(value)
}

fn clean_optional(value: &str, max: usize, label: &str) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.chars().count() > max {
        return Err(format!("{label}不能超过 {max} 个字符"));
    }
    Ok(value)
}

fn clean_optional_option(
    value: Option<String>,
    max: usize,
    label: &str,
) -> Result<Option<String>, String> {
    value
        .map(|value| clean_optional(&value, max, label))
        .transpose()
        .map(|value| value.filter(|item| !item.is_empty()))
}

fn parse_uuid(value: &str, label: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| format!("{label}无效"))
}

fn parse_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn utc_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn db_error(error: sqlx::Error) -> String {
    format!("本地数据库操作失败：{error}")
}

fn json_error(error: serde_json::Error) -> String {
    format!("数据格式错误：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objective_answers_are_order_independent_and_trimmed() {
        assert_eq!(
            normalized_answers(&[" B ".to_owned(), "a".to_owned()]),
            normalized_answers(&["A".to_owned(), "b".to_owned()])
        );
    }

    #[test]
    fn default_fsrs_never_schedules_a_zero_day_interval() {
        let states = FSRS::default().next_states(None, 0.9, 0).unwrap();
        assert!(states.again.interval > 0.0);
        assert!(states.good.interval > states.again.interval);
    }
}
