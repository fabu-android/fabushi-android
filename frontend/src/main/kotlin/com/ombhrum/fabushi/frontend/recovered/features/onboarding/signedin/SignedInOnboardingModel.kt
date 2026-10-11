package com.ombhrum.fabushi

internal const val SIGNED_IN_ONBOARDING_HAND_OFF_DWELL_MS = 1_500L
internal const val SIGNED_IN_ONBOARDING_COMPUTER_WAIT_TIMEOUT_MS = 60_000L

internal enum class SignedInOnboardingStep {
    MEET,
    COMPUTER_DEMO,
    JOBS,
    TOOLS,
    CREATE,
    HAND_OFF,
}

internal data class SignedInOnboardingDraft(
    val name: String = "",
    val description: String = "",
    val color: String = "blue",
    val shape: String = "blob",
    val pickedTemplateId: String? = null,
)

internal data class SignedInOnboardingSuggestion(
    val templateId: String,
    val name: String,
    val description: String,
)

internal data class SignedInOnboardingUiState(
    val active: Boolean = false,
    val accountSlot: String? = null,
    val step: SignedInOnboardingStep = SignedInOnboardingStep.MEET,
    val draft: SignedInOnboardingDraft = SignedInOnboardingDraft(),
    val selectedDailyTools: Set<String> = emptySet(),
    val creating: Boolean = false,
    val createdAgentId: String? = null,
    val error: String? = null,
)

internal val SIGNED_IN_ONBOARDING_COLORS = linkedMapOf(
    "brown" to "#936439",
    "red" to "#FF263C",
    "orange" to "#FF6700",
    "yellow" to "#FF9800",
    "green" to "#00C972",
    "cyan" to "#00BCA6",
    "blue" to "#1084FE",
    "violet" to "#9159FE",
    "magenta" to "#FF309B",
    "gray" to "#777777",
)

internal val SIGNED_IN_ONBOARDING_SHAPES = listOf(
    "blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop",
)

internal val SIGNED_IN_ONBOARDING_DAILY_TOOLS = listOf(
    "Workspace", "Slack", "Notion", "Salesforce", "Microsoft 365", "LinkedIn", "Zoom",
    "GitHub", "Jira", "Figma", "HubSpot", "Canva", "Trello", "monday.com", "ClickUp",
    "Intercom", "Zendesk", "Box", "Dropbox", "DocuSign", "Calendly", "Loom", "Outreach",
    "Salesloft", "Apollo", "Clay", "ZoomInfo", "Nooks", "Stripe", "Shopify", "QuickBooks",
    "NetSuite", "Ramp", "Workday", "Rippling", "Ashby", "Greenhouse", "Vercel", "Tableau",
    "Hex", "Amplitude", "Mixpanel", "Snowflake", "Databricks", "Mailchimp",
)

private data class SignedInOnboardingSuggestionTemplate(
    val id: String,
    val name: String,
    val description: String,
    val recommendedIf: List<String> = emptyList(),
)

private const val TOOL_TOKEN = "{tool}"

private val SIGNED_IN_ONBOARDING_SUGGESTION_CATALOG = listOf(
    SignedInOnboardingSuggestionTemplate("night-shift", "Night Shift", "Works overnight and preps your morning digest"),
    SignedInOnboardingSuggestionTemplate("inbox-triage", "Inbox Triage", "Sorts your email and drafts replies in your voice"),
    SignedInOnboardingSuggestionTemplate("chief-of-staff", "Chief of Staff", "Manages your other Bots and pulls you in for decisions"),
    SignedInOnboardingSuggestionTemplate("negotiator", "Negotiator", "Researches fair pricing and haggles in your voice"),
    SignedInOnboardingSuggestionTemplate("prototyper", "Prototyper", "Turns your ideas into working prototypes"),
    SignedInOnboardingSuggestionTemplate("researcher", "Researcher", "Digs into any question across your tools and the web"),
    SignedInOnboardingSuggestionTemplate("shopper", "Shopper", "Gathers quotes and options into a clear comparison"),
    SignedInOnboardingSuggestionTemplate("apartment-scout", "Apartment Scout", "Shortlists listings the moment they drop and books tours"),
    SignedInOnboardingSuggestionTemplate("lookout", "Lookout", "Watches any site and alerts you to changes"),
    SignedInOnboardingSuggestionTemplate("competitor-watcher", "Competitor Watcher", "Tracks competitor pricing and launches, and briefs you weekly"),
    SignedInOnboardingSuggestionTemplate("crm-scribe", "CRM Scribe", "Turns your calls into {tool} updates and follow-ups", listOf("Salesforce", "HubSpot", "Outreach", "Salesloft", "Apollo")),
    SignedInOnboardingSuggestionTemplate("pipeline-scout", "Pipeline Scout", "Researches target accounts in {tool} and builds your attack plan", listOf("Salesforce", "HubSpot", "Apollo", "Clay", "ZoomInfo", "LinkedIn", "Outreach", "Salesloft")),
    SignedInOnboardingSuggestionTemplate("first-responder", "First Responder", "Answers new leads in minutes and books the meeting in {tool}", listOf("Calendly", "HubSpot", "Salesforce", "Intercom", "Outreach", "Salesloft", "Apollo")),
    SignedInOnboardingSuggestionTemplate("win-loss-analyst", "Win-Loss Analyst", "Reads every lost deal in {tool} and reports why you’re really losing", listOf("Salesforce", "HubSpot", "Outreach", "Salesloft", "Apollo")),
    SignedInOnboardingSuggestionTemplate("icebreaker", "Icebreaker", "Watches {tool} for launches and hires worth a warm intro", listOf("LinkedIn", "Clay", "ZoomInfo", "Apollo", "Salesforce", "HubSpot")),
    SignedInOnboardingSuggestionTemplate("call-coach", "Call Coach", "Rewatches your {tool} calls and gives specific coaching", listOf("Zoom", "Nooks", "Loom", "Outreach", "Salesloft")),
    SignedInOnboardingSuggestionTemplate("deck-designer", "Deck Designer", "Turns your notes into an on-brand {tool} deck", listOf("Canva", "Figma", "Workspace", "Microsoft 365")),
    SignedInOnboardingSuggestionTemplate("channel-digest", "Channel Digest", "Summarizes your {tool} channels and flags what needs you", listOf("Slack", "Microsoft 365")),
    SignedInOnboardingSuggestionTemplate("ticket-triager", "Ticket Triager", "Triages everything new in {tool} and drafts the first reply", listOf("Zendesk", "Intercom", "Jira", "Trello", "monday.com", "ClickUp")),
    SignedInOnboardingSuggestionTemplate("feedback-miner", "Feedback Miner", "Clusters your {tool} feedback into clear themes", listOf("Zendesk", "Intercom", "Shopify", "Notion")),
    SignedInOnboardingSuggestionTemplate("review-responder", "Review Responder", "Drafts on-brand replies to reviews and messages in {tool}", listOf("Shopify", "Zendesk", "Intercom")),
    SignedInOnboardingSuggestionTemplate("marketing-analyst", "Marketing Analyst", "Reports on {tool} campaign performance and where to spend next", listOf("HubSpot", "Mailchimp", "Amplitude", "Mixpanel", "Shopify")),
    SignedInOnboardingSuggestionTemplate("shopkeeper", "Shopkeeper", "Watches orders and payouts in {tool} and flags anything odd", listOf("Shopify", "Stripe")),
    SignedInOnboardingSuggestionTemplate("invoice-chaser", "Invoice Chaser", "Tracks unpaid {tool} invoices and drafts the reminders", listOf("QuickBooks", "NetSuite", "Stripe", "Ramp")),
    SignedInOnboardingSuggestionTemplate("expense-auditor", "Expense Auditor", "Files receipts in {tool} daily and categorizes every charge", listOf("QuickBooks", "NetSuite", "Ramp", "Rippling", "Workday")),
    SignedInOnboardingSuggestionTemplate("subscription-sleuth", "Subscription Sleuth", "Finds subscriptions you no longer use across your {tool} spend", listOf("QuickBooks", "NetSuite", "Ramp", "Stripe")),
    SignedInOnboardingSuggestionTemplate("paralegal", "Paralegal", "Reviews contracts in {tool} and drafts redlines for approval", listOf("DocuSign", "Box", "Dropbox")),
    SignedInOnboardingSuggestionTemplate("application-screener", "Application Screener", "Screens new {tool} applications and surfaces the top candidates", listOf("Ashby", "Greenhouse", "Workday", "Rippling", "LinkedIn")),
    SignedInOnboardingSuggestionTemplate("sourcing-scout", "Sourcing Scout", "Delivers qualified profiles matched to the roles open in {tool}", listOf("Ashby", "Greenhouse", "Workday", "Rippling", "LinkedIn")),
    SignedInOnboardingSuggestionTemplate("qa-engineer", "QA Engineer", "Clicks through every new {tool} deploy and reports what breaks", listOf("Vercel", "GitHub")),
    SignedInOnboardingSuggestionTemplate("dashboard-watcher", "Dashboard Watcher", "Watches your {tool} metrics and alerts you on anomalies", listOf("Tableau", "Hex", "Amplitude", "Mixpanel", "Snowflake", "Databricks", "Stripe", "Shopify")),
    SignedInOnboardingSuggestionTemplate("data-scientist", "Data Scientist", "Answers data questions with real {tool} queries and charts", listOf("Tableau", "Hex", "Amplitude", "Mixpanel", "Snowflake", "Databricks")),
)

internal fun signedInOnboardingShouldOpen(
    signedIn: Boolean,
    hasSeenOnboarding: Boolean,
    agentCount: Int?,
): Boolean =
    signedIn && !hasSeenOnboarding && (agentCount == null || agentCount <= 0)

internal fun nextSignedInOnboardingStep(step: SignedInOnboardingStep): SignedInOnboardingStep =
    when (step) {
        SignedInOnboardingStep.MEET -> SignedInOnboardingStep.COMPUTER_DEMO
        SignedInOnboardingStep.COMPUTER_DEMO -> SignedInOnboardingStep.JOBS
        SignedInOnboardingStep.JOBS -> SignedInOnboardingStep.TOOLS
        SignedInOnboardingStep.TOOLS -> SignedInOnboardingStep.CREATE
        SignedInOnboardingStep.CREATE,
        SignedInOnboardingStep.HAND_OFF -> SignedInOnboardingStep.HAND_OFF
    }

internal fun previousSignedInOnboardingStep(step: SignedInOnboardingStep): SignedInOnboardingStep? =
    when (step) {
        SignedInOnboardingStep.MEET -> null
        SignedInOnboardingStep.COMPUTER_DEMO -> SignedInOnboardingStep.MEET
        SignedInOnboardingStep.JOBS -> SignedInOnboardingStep.COMPUTER_DEMO
        SignedInOnboardingStep.TOOLS -> SignedInOnboardingStep.JOBS
        SignedInOnboardingStep.CREATE -> SignedInOnboardingStep.TOOLS
        SignedInOnboardingStep.HAND_OFF -> null
    }

internal fun normalizeSignedInOnboardingDraft(draft: SignedInOnboardingDraft): SignedInOnboardingDraft =
    draft.copy(
        color = draft.color.takeIf(SIGNED_IN_ONBOARDING_COLORS::containsKey) ?: "blue",
        shape = draft.shape.takeIf(SIGNED_IN_ONBOARDING_SHAPES::contains) ?: "blob",
        pickedTemplateId = draft.pickedTemplateId?.trim()?.take(120)?.takeIf(String::isNotEmpty),
    )

internal fun canSubmitSignedInOnboardingDraft(draft: SignedInOnboardingDraft): Boolean =
    normalizeSignedInOnboardingDraft(draft).name.trim().isNotEmpty()

internal fun signedInOnboardingDescription(
    description: String,
    selectedDailyTools: Collection<String>,
): String {
    val selected = SIGNED_IN_ONBOARDING_DAILY_TOOLS.filter(selectedDailyTools::contains)
    if (selected.isEmpty()) return description
    return description + " The user works with " + selected.joinToString(", ") +
        " every day — start with those tools when suggesting connectors or taking on work."
}

internal fun filterSignedInOnboardingTools(query: String): List<String> {
    val needle = query.trim()
    return if (needle.isEmpty()) SIGNED_IN_ONBOARDING_DAILY_TOOLS
    else SIGNED_IN_ONBOARDING_DAILY_TOOLS.filter { it.contains(needle, ignoreCase = true) }
}

private fun renderOnboardingSuggestion(
    template: SignedInOnboardingSuggestionTemplate,
    tool: String? = null,
): SignedInOnboardingSuggestion =
    SignedInOnboardingSuggestion(
        templateId = template.id,
        name = template.name,
        description = if (tool == null) template.description else template.description.replace(TOOL_TOKEN, tool),
    )

internal fun selectSignedInOnboardingSuggestions(
    selectedDailyTools: Collection<String>,
    limit: Int = 10,
): List<SignedInOnboardingSuggestion> {
    if (limit <= 0) return emptyList()
    val selected = SIGNED_IN_ONBOARDING_DAILY_TOOLS.filter(selectedDailyTools::contains)
    val used = linkedSetOf<String>()
    val result = mutableListOf<SignedInOnboardingSuggestion>()
    for (tool in selected) {
        if (result.size >= limit) break
        val template = SIGNED_IN_ONBOARDING_SUGGESTION_CATALOG
            .filter { it.id !in used && it.recommendedIf.isNotEmpty() }
            .mapNotNull { candidate ->
                candidate.recommendedIf.indexOf(tool).takeIf { it >= 0 }?.let { candidate to it }
            }
            .minByOrNull { it.second }
            ?.first
        if (template != null) {
            used += template.id
            result += renderOnboardingSuggestion(template, tool)
        }
    }
    for (template in SIGNED_IN_ONBOARDING_SUGGESTION_CATALOG) {
        if (result.size >= limit) break
        if (template.id in used || template.recommendedIf.isEmpty()) continue
        val tool = template.recommendedIf.firstOrNull(selected::contains) ?: continue
        used += template.id
        result += renderOnboardingSuggestion(template, tool)
    }
    for (template in SIGNED_IN_ONBOARDING_SUGGESTION_CATALOG) {
        if (result.size >= limit) break
        if (template.id !in used && template.recommendedIf.isEmpty()) {
            used += template.id
            result += renderOnboardingSuggestion(template)
        }
    }
    return result
}

internal fun signedInOnboardingHandoffStatus(
    isComputerReady: Boolean,
    pullPercent: Int?,
    computerState: String?,
): String =
    when {
        isComputerReady -> "Getting your team ready…"
        pullPercent != null -> "Setting up your Grok Bot… " + pullPercent.coerceIn(0, 100) + "%"
        computerState == "hibernated" -> "Waking your computer…"
        else -> "Setting up your Grok Bot…"
    }

internal data class SignedInOnboardingOperationToken(
    val generation: Long,
    val accountSlot: String,
)

internal class SignedInOnboardingOperationFence {
    private var generation = 0L
    private var accountSlot: String? = null
    private var pending: SignedInOnboardingOperationToken? = null
    private var disposed = false

    fun setAccount(next: String?) {
        if (disposed || accountSlot == next) return
        generation += 1
        pending = null
        accountSlot = next
    }

    fun begin(): SignedInOnboardingOperationToken? {
        val account = accountSlot ?: return null
        if (disposed || pending != null || account.isBlank()) return null
        generation += 1
        return SignedInOnboardingOperationToken(generation, account).also { pending = it }
    }

    fun accepts(token: SignedInOnboardingOperationToken, currentAccountSlot: String?): Boolean =
        !disposed && token == pending && token.generation == generation && token.accountSlot == currentAccountSlot

    fun finish(token: SignedInOnboardingOperationToken) {
        if (pending == token) pending = null
    }

    fun dispose() {
        disposed = true
        generation += 1
        pending = null
        accountSlot = null
    }
}
