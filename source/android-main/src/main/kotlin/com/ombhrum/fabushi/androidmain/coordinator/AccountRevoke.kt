package com.ombhrum.fabushi.androidmain.coordinator
data class RefusedAccount(val accountId: String, val reason: String)
interface AccountRevocationService { fun revoke(accountId: String): Boolean }
fun revokeRefusedAccount(refused: RefusedAccount, service: AccountRevocationService): Boolean {
    require(refused.accountId.isNotBlank())
    return service.revoke(refused.accountId)
}
