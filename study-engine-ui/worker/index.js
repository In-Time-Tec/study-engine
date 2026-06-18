const BUILTIN_BANK_NAMES = ['cca-f']
const JSON_HEADERS = {
  'content-type': 'application/json; charset=utf-8',
  'access-control-allow-origin': '*',
  'access-control-allow-methods': 'GET, POST, DELETE, OPTIONS',
  'access-control-allow-headers': 'content-type'
}

const SCHEMA = [
  `CREATE TABLE IF NOT EXISTS cards (
    user_key TEXT NOT NULL,
    id TEXT NOT NULL,
    cert TEXT NOT NULL,
    stability REAL,
    difficulty REAL,
    due TEXT,
    last_review TEXT,
    reps INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_key, cert, id)
  )`,
  `CREATE TABLE IF NOT EXISTS reviews (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_key TEXT NOT NULL,
    card_id TEXT NOT NULL,
    cert TEXT NOT NULL,
    ts TEXT NOT NULL,
    correct INTEGER NOT NULL,
    rating INTEGER NOT NULL,
    selected_letter TEXT
  )`,
  `CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_key TEXT NOT NULL,
    cert TEXT NOT NULL,
    date TEXT NOT NULL,
    total INTEGER NOT NULL,
    correct INTEGER NOT NULL
  )`,
  `CREATE TABLE IF NOT EXISTS pending_sessions (
    user_key TEXT NOT NULL,
    cert TEXT NOT NULL,
    card_ids TEXT NOT NULL,
    control_mode TEXT NOT NULL DEFAULT 'due',
    control_domain INTEGER,
    started_at TEXT NOT NULL,
    PRIMARY KEY (user_key, cert)
  )`,
  `CREATE TABLE IF NOT EXISTS banks (
    user_key TEXT NOT NULL,
    name TEXT NOT NULL,
    content TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_key, name)
  )`,
  `CREATE TABLE IF NOT EXISTS hidden_banks (
    user_key TEXT NOT NULL,
    name TEXT NOT NULL,
    PRIMARY KEY (user_key, name)
  )`,
  'CREATE INDEX IF NOT EXISTS reviews_user_cert_ts_idx ON reviews (user_key, cert, ts)',
  'CREATE INDEX IF NOT EXISTS sessions_user_cert_id_idx ON sessions (user_key, cert, id)'
]

let schemaReady

class ApiError extends Error {
  constructor(status, message) {
    super(message)
    this.status = status
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url)

    if (request.method === 'OPTIONS') {
      return new Response(null, { status: 204, headers: JSON_HEADERS })
    }

    if (url.pathname.startsWith('/api/')) {
      try {
        if (!env.DB) throw new ApiError(500, 'D1 binding DB is not configured')
        await ensureSchema(env.DB)
        return await routeApi(request, env, url)
      } catch (error) {
        const status = error instanceof ApiError ? error.status : 500
        const message = error instanceof Error ? error.message : 'Unknown error'
        return json({ error: message }, status)
      }
    }

    return serveAsset(request, env)
  }
}

async function ensureSchema(db) {
  schemaReady ||= (async () => {
    for (const sql of SCHEMA) {
      await db.prepare(sql).run()
    }
  })()
  return schemaReady
}

async function routeApi(request, env, url) {
  const user = userKey(request)
  const path = url.pathname

  if (path === '/api/certs' && request.method === 'GET') {
    return json({ certs: await listCertNames(env.DB, user) })
  }

  if (path === '/api/banks' && request.method === 'GET') {
    const banks = await Promise.all(
      (await listCertNames(env.DB, user)).map(async (name) => {
        const bank = await loadBank(env, request, user, name)
        return { name, questionCount: bank.questions.length }
      })
    )
    return json({ banks })
  }

  if (path === '/api/banks' && request.method === 'POST') {
    return uploadBank(request, env, user)
  }

  if (path.startsWith('/api/banks/') && request.method === 'DELETE') {
    const name = sanitizeCertName(decodeURIComponent(path.slice('/api/banks/'.length)))
    return deleteBank(env.DB, user, name)
  }

  if (path === '/api/stats' && request.method === 'GET') {
    return getStats(request, env, user, url)
  }

  if (path === '/api/due' && request.method === 'GET') {
    return getDue(request, env, user, url)
  }

  if (path === '/api/questions' && request.method === 'GET') {
    return getQuestions(request, env, user, url)
  }

  if (path === '/api/review' && request.method === 'POST') {
    return postReview(request, env, user)
  }

  if (path === '/api/session' && request.method === 'POST') {
    return postSession(request, env.DB, user)
  }

  if (path === '/api/sessions' && request.method === 'GET') {
    return getSessions(env.DB, user, url)
  }

  if (path === '/api/pending-session' && request.method === 'POST') {
    return postPendingSession(request, env.DB, user)
  }

  if (path === '/api/pending-session' && request.method === 'GET') {
    return getPendingSession(request, env, user, url)
  }

  if (path === '/api/pending-session' && request.method === 'DELETE') {
    return deletePendingSession(env.DB, user, url)
  }

  throw new ApiError(404, 'Not found')
}

async function getStats(request, env, user, url) {
  const cert = certParam(url)
  const bank = await loadBank(env, request, user, cert)
  const cards = await allCards(env.DB, user, cert)
  const reviews = await allReviews(env.DB, user, cert)
  const sessions = await recentSessions(env.DB, user, cert, 5)
  const summary = summarizeProgress(bank, bank.questions, cards, reviews, sessions, todayString())

  return json({
    cert,
    certName: bank.name,
    total: summary.total,
    introduced: summary.introduced,
    dueToday: summary.dueToday,
    nextDue: summary.nextDue,
    newAvailable: summary.newAvailable,
    mastered: summary.mastered,
    domains: summary.domains,
    tags: summary.tags,
    sessions: summary.sessions
  })
}

async function getDue(request, env, user, url) {
  const cert = certParam(url)
  const maxNew = intParam(url, 'new', 5)
  const domain = nullableIntParam(url, 'domain')
  const tag = url.searchParams.get('tag')
  const ids = url.searchParams.get('ids')
  const all = url.searchParams.get('all') === 'true'
  const bank = await loadBank(env, request, user, cert)
  const cards = await allCards(env.DB, user, cert)
  const cardMap = new Map(cards.map((card) => [card.id, card]))

  if (all) {
    const questions = shuffle(bankFilter(bank, domain, tag))
    return json({
      cards: questions.map((question) => cardWithQuestion(question, cardMap)),
      dueCount: 0,
      newCount: questions.length,
      newRemaining: 0,
      mode: 'all',
      glossary: bank.glossary
    })
  }

  if (ids) {
    const idSet = new Set(ids.split(',').filter(Boolean))
    const questions = bank.questions.filter((question) => idSet.has(question.id))
    return json({
      cards: questions.map((question) => cardWithQuestion(question, cardMap)),
      dueCount: 0,
      newCount: questions.length,
      newRemaining: 0,
      mode: 'quiz',
      glossary: bank.glossary
    })
  }

  const plan = planStudySession(bankFilter(bank, domain, tag), cardMap, todayString(), maxNew)
  return json({
    dueCount: plan.due.length,
    newCount: plan.new.length - plan.newRemaining,
    newRemaining: plan.newRemaining,
    mode: 'study',
    cards: plan.session.map((question) => cardWithQuestion(question, cardMap)),
    glossary: bank.glossary
  })
}

async function getQuestions(request, env, user, url) {
  const cert = certParam(url)
  const domain = nullableIntParam(url, 'domain')
  const tag = url.searchParams.get('tag')
  const search = url.searchParams.get('search')?.toLowerCase()
  const bank = await loadBank(env, request, user, cert)
  const cards = await allCards(env.DB, user, cert)
  const cardMap = new Map(cards.map((card) => [card.id, card]))
  let questions = bankFilter(bank, domain, tag)

  if (search) {
    questions = questions.filter((question) =>
      [question.question, question.scenario, question.explanation].some((text) =>
        String(text ?? '').toLowerCase().includes(search)
      )
    )
  }

  return json({
    cert,
    certName: bank.name,
    domains: bank.domains,
    questions: questions.map((question) => cardWithQuestion(question, cardMap)),
    glossary: bank.glossary
  })
}

async function postReview(request, env, user) {
  const body = await jsonBody(request)
  const cert = body.cert || 'cca-f'
  const cardId = String(body.cardId ?? '')
  const rating = Number(body.rating)
  const isCorrect = Boolean(body.isCorrect)
  validateReviewRating(isCorrect, rating)

  const bank = await loadBank(env, request, user, cert)
  if (!bank.questions.some((question) => question.id === cardId)) {
    throw new ApiError(400, `Unknown question ID for cert ${cert}: ${cardId}`)
  }

  const card = await getCard(env.DB, user, cert, cardId)
  const scheduled = scheduleNext(card, rating, new Date())
  const updated = {
    ...card,
    stability: scheduled.stability,
    difficulty: scheduled.difficulty,
    due: scheduled.due,
    last_review: isoTimestamp(),
    reps: isCorrect ? Number(card.reps || 0) + 1 : 0
  }

  await env.DB.batch([
    env.DB.prepare(
      `INSERT OR REPLACE INTO cards
       (user_key, id, cert, stability, difficulty, due, last_review, reps)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)`
    ).bind(
      user,
      updated.id,
      updated.cert,
      updated.stability,
      updated.difficulty,
      updated.due,
      updated.last_review,
      updated.reps
    ),
    env.DB.prepare(
      `INSERT INTO reviews
       (user_key, card_id, cert, ts, correct, rating, selected_letter)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)`
    ).bind(user, cardId, cert, isoTimestamp(), isCorrect ? 1 : 0, rating, body.selected ?? null)
  ])

  return json({ ok: true })
}

async function postSession(request, db, user) {
  const body = await jsonBody(request)
  const total = Number(body.total)
  const correct = Number(body.correct)
  if (correct > total) throw new ApiError(400, 'Session correct count cannot exceed total')

  await db
    .prepare('INSERT INTO sessions (user_key, cert, date, total, correct) VALUES (?1, ?2, ?3, ?4, ?5)')
    .bind(user, body.cert || 'cca-f', todayString(), total, correct)
    .run()

  return json({ ok: true })
}

async function getSessions(db, user, url) {
  const cert = certParam(url)
  const limit = intParam(url, 'limit', 30)
  return json({ sessions: await recentSessions(db, user, cert, limit) })
}

async function postPendingSession(request, db, user) {
  const body = await jsonBody(request)
  const cert = body.cert || 'cca-f'
  await db
    .prepare(
      `INSERT OR REPLACE INTO pending_sessions
       (user_key, cert, card_ids, control_mode, control_domain, started_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6)`
    )
    .bind(
      user,
      cert,
      JSON.stringify(Array.isArray(body.cardIds) ? body.cardIds : []),
      body.controlMode || 'due',
      body.controlDomain ?? null,
      isoTimestamp()
    )
    .run()

  return json({ ok: true })
}

async function getPendingSession(request, env, user, url) {
  const cert = certParam(url)
  const row = await env.DB
    .prepare(
      `SELECT card_ids, control_mode, control_domain, started_at
       FROM pending_sessions WHERE user_key = ?1 AND cert = ?2`
    )
    .bind(user, cert)
    .first()

  if (!row) throw new ApiError(404, 'no pending session for this cert')

  const cardIds = JSON.parse(row.card_ids)
  const bank = await loadBank(env, request, user, cert)
  const questionMap = new Map(bank.questions.map((question) => [question.id, question]))
  const reviews = await reviewsSince(env.DB, user, cert, cardIds, row.started_at)

  return json({
    cardIds,
    controlMode: row.control_mode,
    controlDomain: row.control_domain,
    reviewedCards: reviews.flatMap((review) => {
      const question = questionMap.get(review.card_id)
      if (!question) return []
      return [{
        cardId: review.card_id,
        isCorrect: Boolean(review.correct),
        rating: review.rating,
        selectedLetter: review.selected_letter,
        domain: question.domain,
        correctAnswer: question.answer,
        questionText: question.question
      }]
    })
  })
}

async function deletePendingSession(db, user, url) {
  await db
    .prepare('DELETE FROM pending_sessions WHERE user_key = ?1 AND cert = ?2')
    .bind(user, certParam(url))
    .run()
  return json({ ok: true })
}

async function uploadBank(request, env, user) {
  const body = await jsonBody(request)
  const name = sanitizeCertName(String(body.name ?? ''))
  const content = String(body.content ?? '')
  parseBank(content)

  const exists = await bankExists(env.DB, user, name)
  if (exists && !body.overwrite) {
    throw new ApiError(
      409,
      `A bank named '${name}' already exists. Replacing it may orphan saved progress if its question IDs changed.`
    )
  }

  await env.DB.batch([
    env.DB.prepare(
      'INSERT OR REPLACE INTO banks (user_key, name, content, updated_at) VALUES (?1, ?2, ?3, ?4)'
    ).bind(user, name, content, isoTimestamp()),
    env.DB.prepare('DELETE FROM hidden_banks WHERE user_key = ?1 AND name = ?2').bind(user, name)
  ])

  return json({ certs: await listCertNames(env.DB, user) })
}

async function deleteBank(db, user, name) {
  const uploaded = await db
    .prepare('SELECT 1 FROM banks WHERE user_key = ?1 AND name = ?2')
    .bind(user, name)
    .first()
  const builtin = BUILTIN_BANK_NAMES.includes(name)

  if (!uploaded && !builtin) throw new ApiError(400, `No bank named '${name}'`)

  const statements = [
    db.prepare('DELETE FROM banks WHERE user_key = ?1 AND name = ?2').bind(user, name)
  ]
  if (builtin) {
    statements.push(
      db.prepare('INSERT OR REPLACE INTO hidden_banks (user_key, name) VALUES (?1, ?2)').bind(user, name)
    )
  }
  await db.batch(statements)

  return json({ certs: await listCertNames(db, user) })
}

async function listCertNames(db, user) {
  const hidden = new Set((await all(db.prepare('SELECT name FROM hidden_banks WHERE user_key = ?1').bind(user))).map((r) => r.name))
  const uploaded = await all(db.prepare('SELECT name FROM banks WHERE user_key = ?1').bind(user))
  const names = new Set(BUILTIN_BANK_NAMES.filter((name) => !hidden.has(name)))
  for (const row of uploaded) names.add(row.name)
  return [...names].sort()
}

async function bankExists(db, user, name) {
  if (BUILTIN_BANK_NAMES.includes(name)) {
    const hidden = await db
      .prepare('SELECT 1 FROM hidden_banks WHERE user_key = ?1 AND name = ?2')
      .bind(user, name)
      .first()
    if (!hidden) return true
  }
  return Boolean(
    await db.prepare('SELECT 1 FROM banks WHERE user_key = ?1 AND name = ?2').bind(user, name).first()
  )
}

async function loadBank(env, request, user, cert) {
  const uploaded = await env.DB
    .prepare('SELECT content FROM banks WHERE user_key = ?1 AND name = ?2')
    .bind(user, cert)
    .first()
  if (uploaded) return parseBank(uploaded.content)

  if (BUILTIN_BANK_NAMES.includes(cert)) {
    const hidden = await env.DB
      .prepare('SELECT 1 FROM hidden_banks WHERE user_key = ?1 AND name = ?2')
      .bind(user, cert)
      .first()
    if (!hidden) {
      const assetUrl = new URL(`/questions/${cert}.json`, request.url)
      const response = await env.ASSETS.fetch(new Request(assetUrl))
      if (response.ok) return parseBank(await response.text())
    }
  }

  throw new ApiError(400, `No bank named '${cert}'`)
}

function parseBank(raw) {
  let bank
  try {
    bank = JSON.parse(raw)
  } catch {
    throw new ApiError(400, 'Failed to parse question JSON')
  }

  bank.glossary ||= []
  if (!bank.domains || typeof bank.domains !== 'object') throw new ApiError(400, 'Bank domains must be an object')
  if (!Array.isArray(bank.questions)) throw new ApiError(400, 'Bank questions must be an array')
  if (!Array.isArray(bank.glossary)) throw new ApiError(400, 'Bank glossary must be an array')

  const surfaces = new Set()
  for (const entry of bank.glossary) {
    if (!String(entry.term ?? '').trim()) throw new ApiError(400, 'Glossary terms cannot be empty')
    if (!String(entry.definition ?? '').trim()) {
      throw new ApiError(400, `Glossary term '${entry.term}' has an empty definition`)
    }
    if (!String(entry.sourceUrl ?? '').startsWith('https://') && !String(entry.sourceUrl ?? '').startsWith('http://')) {
      throw new ApiError(400, `Glossary term '${entry.term}' source URL must start with http:// or https://`)
    }
    entry.aliases ||= []
    for (const surface of [entry.term, ...entry.aliases]) {
      if (!String(surface ?? '').trim()) throw new ApiError(400, `Glossary term '${entry.term}' has an empty alias`)
      const key = String(surface).trim().toLowerCase()
      if (surfaces.has(key)) throw new ApiError(400, `Duplicate glossary term or alias: ${surface}`)
      surfaces.add(key)
    }
    entry.sourceTitle ??= null
  }

  const ids = new Set()
  for (const question of bank.questions) {
    if (!String(question.id ?? '').trim()) throw new ApiError(400, 'Question IDs cannot be empty')
    if (ids.has(question.id)) throw new ApiError(400, `Duplicate question ID: ${question.id}`)
    ids.add(question.id)
    if (!Object.prototype.hasOwnProperty.call(bank.domains, String(question.domain))) {
      throw new ApiError(400, `Question ${question.id} references unknown domain ${question.domain}`)
    }
    if (!question.options || typeof question.options !== 'object' || Object.keys(question.options).length === 0) {
      throw new ApiError(400, `Question ${question.id} must have at least one option`)
    }
    if (!Object.prototype.hasOwnProperty.call(question.options, question.answer)) {
      throw new ApiError(400, `Question ${question.id} answer ${question.answer} is not in options`)
    }
    question.tags ||= []
    question.glossaryExclude ||= []
    for (const exclude of question.glossaryExclude) {
      if (!surfaces.has(String(exclude).trim().toLowerCase())) {
        throw new ApiError(400, `Question ${question.id} excludes unknown glossary term '${exclude}'`)
      }
    }
    shuffleOptions(question)
  }

  return bank
}

function shuffleOptions(question) {
  const keys = Object.keys(question.options).sort()
  const encoder = new TextEncoder()
  let state = 0xcbf29ce484222325n
  for (const byte of [...keys.flatMap((key) => [...encoder.encode(key)]), ...encoder.encode(question.id)]) {
    state = u64((state ^ BigInt(byte)) * 0x100000001b3n)
  }
  state |= 1n

  function next() {
    state = u64(state ^ (state >> 12n))
    state = u64(state ^ (state << 25n))
    state = u64(state ^ (state >> 27n))
    return u64(state * 0x2545f4914f6cdd1dn)
  }

  const permutation = [...keys]
  for (let i = permutation.length - 1; i > 0; i -= 1) {
    const j = Number(next() % BigInt(i + 1))
    ;[permutation[i], permutation[j]] = [permutation[j], permutation[i]]
  }

  const nextOptions = {}
  let nextAnswer = question.answer
  for (let i = 0; i < keys.length; i += 1) {
    nextOptions[keys[i]] = question.options[permutation[i]]
    if (permutation[i] === question.answer) nextAnswer = keys[i]
  }
  question.options = nextOptions
  question.answer = nextAnswer
}

function u64(value) {
  return BigInt.asUintN(64, value)
}

async function allCards(db, user, cert) {
  const rows = await all(
    db
      .prepare(
        `SELECT id, cert, stability, difficulty, due, last_review, reps
         FROM cards WHERE user_key = ?1 AND cert = ?2`
      )
      .bind(user, cert)
  )
  return rows.map(cardFromRow)
}

async function getCard(db, user, cert, id) {
  const row = await db
    .prepare(
      `SELECT id, cert, stability, difficulty, due, last_review, reps
       FROM cards WHERE user_key = ?1 AND cert = ?2 AND id = ?3`
    )
    .bind(user, cert, id)
    .first()
  return row ? cardFromRow(row) : { id, cert, stability: null, difficulty: null, due: null, last_review: null, reps: 0 }
}

function cardFromRow(row) {
  return {
    id: row.id,
    cert: row.cert,
    stability: row.stability,
    difficulty: row.difficulty,
    due: row.due,
    last_review: row.last_review,
    reps: row.reps
  }
}

async function allReviews(db, user, cert) {
  return all(
    db
      .prepare('SELECT card_id, correct, rating FROM reviews WHERE user_key = ?1 AND cert = ?2')
      .bind(user, cert)
  )
}

async function reviewsSince(db, user, cert, cardIds, startedAt) {
  if (cardIds.length === 0) return []
  const placeholders = cardIds.map((_, index) => `?${index + 4}`).join(',')
  return all(
    db
      .prepare(
        `SELECT card_id, correct, rating, selected_letter
         FROM reviews
         WHERE user_key = ?1 AND cert = ?2 AND ts >= ?3 AND card_id IN (${placeholders})
         ORDER BY ts`
      )
      .bind(user, cert, startedAt, ...cardIds)
  )
}

async function recentSessions(db, user, cert, limit) {
  const rows = await all(
    db
      .prepare(
        `SELECT date, total, correct FROM sessions
         WHERE user_key = ?1 AND cert = ?2 ORDER BY id DESC LIMIT ?3`
      )
      .bind(user, cert, limit)
  )
  return rows.map((row) => ({
    date: row.date,
    total: row.total,
    correct: row.correct,
    accuracy: accuracy(row.correct, row.total)
  }))
}

function summarizeProgress(bank, questions, cards, reviews, sessions, today) {
  const cardMap = new Map(cards.map((card) => [card.id, card]))
  const reviewsByCard = new Map()
  for (const review of reviews) {
    const current = reviewsByCard.get(review.card_id) || { correct: 0, total: 0 }
    current.total += 1
    if (review.correct) current.correct += 1
    reviewsByCard.set(review.card_id, current)
  }

  const introduced = questions.filter((q) => cardMap.get(q.id)?.due).length
  const dueToday = questions.filter((q) => {
    const due = cardMap.get(q.id)?.due
    return due && due <= today
  }).length
  const futureDue = questions
    .map((q) => cardMap.get(q.id)?.due)
    .filter((due) => due && due > today)
    .sort()
  const mastered = questions.filter((q) => Number(cardMap.get(q.id)?.reps || 0) >= 3).length

  const domainIds = Object.keys(bank.domains)
    .map((id) => Number(id))
    .filter((id) => Number.isFinite(id))
    .sort((a, b) => a - b)
  const domains = domainIds.map((id) => {
    const domainQuestions = questions.filter((q) => Number(q.domain) === id)
    const totals = domainQuestions.reduce(
      (acc, q) => {
        const reviewsForQuestion = reviewsByCard.get(q.id) || { correct: 0, total: 0 }
        acc.reviewCorrect += reviewsForQuestion.correct
        acc.reviewTotal += reviewsForQuestion.total
        if (Number(cardMap.get(q.id)?.reps || 0) >= 3) acc.mastered += 1
        return acc
      },
      { reviewCorrect: 0, reviewTotal: 0, mastered: 0 }
    )
    return {
      id,
      name: domainName(bank, id),
      total: domainQuestions.length,
      mastered: totals.mastered,
      reviewTotal: totals.reviewTotal,
      reviewCorrect: totals.reviewCorrect,
      accuracy: accuracy(totals.reviewCorrect, totals.reviewTotal)
    }
  })

  const tagStats = new Map()
  for (const question of questions) {
    const totals = reviewsByCard.get(question.id)
    if (!totals) continue
    for (const tag of question.tags || []) {
      const current = tagStats.get(tag) || { correct: 0, total: 0 }
      current.correct += totals.correct
      current.total += totals.total
      tagStats.set(tag, current)
    }
  }
  const tags = [...tagStats.entries()]
    .filter(([, value]) => value.total >= 1)
    .map(([tag, value]) => ({ tag, correct: value.correct, total: value.total, accuracy: accuracy(value.correct, value.total) }))
    .sort((a, b) => a.accuracy - b.accuracy)

  return {
    total: questions.length,
    introduced,
    dueToday,
    nextDue: dueToday > 0 ? today : futureDue[0] || null,
    newAvailable: questions.length - introduced,
    mastered,
    domains,
    tags,
    sessions
  }
}

function planStudySession(questions, cardMap, today, maxNew) {
  const due = []
  const fresh = []
  for (const question of questions) {
    const card = cardMap.get(question.id)
    if (!card || !card.due) fresh.push(question)
    else if (card.due <= today) due.push(question)
  }
  const dueInterleaved = interleaveByDomain(due)
  const newInterleaved = interleaveByDomain(fresh)
  const newCount = Math.min(maxNew, newInterleaved.length)
  return {
    due: dueInterleaved,
    new: newInterleaved,
    newRemaining: newInterleaved.length - newCount,
    session: [...dueInterleaved, ...newInterleaved.slice(0, newCount)]
  }
}

function interleaveByDomain(questions) {
  const byDomain = new Map()
  for (const question of questions) {
    const key = Number(question.domain)
    if (!byDomain.has(key)) byDomain.set(key, [])
    byDomain.get(key).push(question)
  }
  const result = []
  while (byDomain.size > 0) {
    for (const key of [...byDomain.keys()].sort((a, b) => a - b)) {
      const bucket = byDomain.get(key)
      const next = bucket.shift()
      if (next) result.push(next)
      if (bucket.length === 0) byDomain.delete(key)
    }
  }
  return result
}

function scheduleNext(card, rating, date) {
  const reps = Number(card.reps || 0)
  const interval =
    rating === 1 ? 1 : rating === 3 ? Math.max(2, Math.round((reps + 1) * 2.5)) : Math.max(4, Math.round((reps + 1) * 4))
  const stabilityBase = Number(card.stability || 1)
  const difficultyBase = Number(card.difficulty || 5)
  return {
    stability: Math.max(1, stabilityBase + (rating === 4 ? 1.2 : rating === 3 ? 0.6 : -0.4)),
    difficulty: Math.min(10, Math.max(1, difficultyBase + (rating === 1 ? 0.8 : rating === 3 ? -0.1 : -0.4))),
    due: addDays(date, interval)
  }
}

function bankFilter(bank, domain, tag) {
  return bank.questions.filter((question) => {
    if (domain !== null && Number(question.domain) !== domain) return false
    if (tag && !(question.tags || []).includes(tag)) return false
    return true
  })
}

function cardWithQuestion(question, cardMap) {
  return { question, cardState: cardMap.get(question.id) || null }
}

function domainName(bank, domain) {
  return bank.domains[String(domain)] || 'Unknown'
}

function accuracy(correct, total) {
  return total > 0 ? Math.floor((correct * 100) / total) : 0
}

function validateReviewRating(isCorrect, rating) {
  if (isCorrect && (rating === 3 || rating === 4)) return
  if (!isCorrect && rating === 1) return
  throw new ApiError(
    400,
    isCorrect ? 'Correct reviews must use rating 3 (Good) or 4 (Easy)' : 'Incorrect reviews must use rating 1 (Again)'
  )
}

function sanitizeCertName(name) {
  const trimmed = name.trim()
  if (!trimmed) throw new ApiError(400, 'Bank name cannot be empty')
  if (!/^[A-Za-z0-9_-]+$/.test(trimmed)) {
    throw new ApiError(400, 'Bank name may only contain letters, numbers, hyphens, and underscores')
  }
  return trimmed
}

async function serveAsset(request, env) {
  const response = await env.ASSETS.fetch(request)
  if (response.status !== 404 || request.method !== 'GET') return response

  const url = new URL(request.url)
  if (url.pathname.includes('.')) return response
  return env.ASSETS.fetch(new Request(new URL('/index.html', url)))
}

function json(data, status = 200) {
  return new Response(JSON.stringify(data), { status, headers: JSON_HEADERS })
}

async function jsonBody(request) {
  try {
    return await request.json()
  } catch {
    throw new ApiError(400, 'Request body must be valid JSON')
  }
}

async function all(statement) {
  const result = await statement.all()
  return result.results || []
}

function certParam(url) {
  return url.searchParams.get('cert') || 'cca-f'
}

function intParam(url, name, fallback) {
  const value = Number(url.searchParams.get(name) ?? fallback)
  return Number.isFinite(value) ? value : fallback
}

function nullableIntParam(url, name) {
  const raw = url.searchParams.get(name)
  if (raw === null || raw === '') return null
  const value = Number(raw)
  return Number.isFinite(value) ? value : null
}

function userKey(request) {
  return (
    request.headers.get('oai-authenticated-user-email') ||
    request.headers.get('cf-access-authenticated-user-email') ||
    'anonymous'
  ).toLowerCase()
}

function todayString() {
  return new Date().toISOString().slice(0, 10)
}

function isoTimestamp() {
  return new Date().toISOString().replace(/\.\d{3}Z$/, '')
}

function addDays(date, days) {
  const next = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()))
  next.setUTCDate(next.getUTCDate() + days)
  return next.toISOString().slice(0, 10)
}

function shuffle(items) {
  const result = [...items]
  for (let i = result.length - 1; i > 0; i -= 1) {
    const j = Math.floor(Math.random() * (i + 1))
    ;[result[i], result[j]] = [result[j], result[i]]
  }
  return result
}
