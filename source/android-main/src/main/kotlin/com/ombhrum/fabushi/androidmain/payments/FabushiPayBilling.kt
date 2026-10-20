package com.ombhrum.fabushi

import android.app.Activity
import com.android.billingclient.api.AcknowledgePurchaseParams
import com.android.billingclient.api.BillingClient
import com.android.billingclient.api.BillingClientStateListener
import com.android.billingclient.api.BillingFlowParams
import com.android.billingclient.api.BillingResult
import com.android.billingclient.api.ConsumeParams
import com.android.billingclient.api.PendingPurchasesParams
import com.android.billingclient.api.ProductDetails
import com.android.billingclient.api.Purchase
import com.android.billingclient.api.PurchasesUpdatedListener
import com.android.billingclient.api.QueryProductDetailsParams
import java.net.HttpURLConnection
import java.net.URL
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONObject
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

internal data class FabushiPayReceipt(
    val paymentId: String,
    val sku: String,
    val rail: String,
    val amount: Long,
    val currency: String,
    val status: String,
)

internal class FabushiPayBilling(
    private val activity: Activity,
    private val accessTokenProvider: suspend () -> String,
    private val serviceBaseUrl: String = "https://pay.ombhrum.com",
    private val onResult: (Result<FabushiPayReceipt>) -> Unit,
) : PurchasesUpdatedListener {
    private data class PendingPayment(
        val paymentId: String,
        val productId: String,
        val verifyPath: String,
        val consumable: Boolean,
    )

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val pendingByPaymentId = mutableMapOf<String, PendingPayment>()

    private val billingClient: BillingClient = BillingClient.newBuilder(activity)
        .setListener(this)
        .enablePendingPurchases(
            PendingPurchasesParams.newBuilder()
                .enableOneTimeProducts()
                .build(),
        )
        .enableAutoServiceReconnection()
        .build()

    suspend fun connect() {
        if (billingClient.isReady) return
        suspendCancellableCoroutine<Unit> { continuation ->
            billingClient.startConnection(object : BillingClientStateListener {
                override fun onBillingSetupFinished(result: BillingResult) {
                    if (result.responseCode == BillingClient.BillingResponseCode.OK) {
                        continuation.resume(Unit)
                    } else {
                        continuation.resumeWithException(
                            IllegalStateException("Play Billing setup failed: ${result.debugMessage}"),
                        )
                    }
                }

                override fun onBillingServiceDisconnected() {
                    // Billing 8+ auto reconnection is enabled; the next API call reconnects.
                }
            })
        }
    }

    suspend fun launch(
        paymentId: String,
        productId: String,
        verifyPath: String,
        subscription: Boolean,
        consumable: Boolean,
    ): BillingResult {
        require(paymentId.length in 1..64) { "paymentId must fit Google obfuscatedAccountId" }
        connect()
        val productType = if (subscription) BillingClient.ProductType.SUBS else BillingClient.ProductType.INAPP
        val details = queryProduct(productId, productType)
        val productParams = BillingFlowParams.ProductDetailsParams.newBuilder()
            .setProductDetails(details)
            .apply {
                if (subscription) {
                    val offerToken = details.subscriptionOfferDetails
                        ?.firstOrNull()
                        ?.offerToken
                        ?: error("No purchasable Google Play subscription offer")
                    setOfferToken(offerToken)
                }
            }
            .build()
        pendingByPaymentId[paymentId] = PendingPayment(
            paymentId = paymentId,
            productId = productId,
            verifyPath = verifyPath,
            consumable = consumable,
        )
        val params = BillingFlowParams.newBuilder()
            .setProductDetailsParamsList(listOf(productParams))
            .setObfuscatedAccountId(paymentId)
            .build()
        val result = billingClient.launchBillingFlow(activity, params)
        if (result.responseCode != BillingClient.BillingResponseCode.OK) {
            pendingByPaymentId.remove(paymentId)
        }
        return result
    }

    override fun onPurchasesUpdated(result: BillingResult, purchases: MutableList<Purchase>?) {
        if (result.responseCode == BillingClient.BillingResponseCode.USER_CANCELED) {
            return
        }
        if (result.responseCode != BillingClient.BillingResponseCode.OK || purchases == null) {
            onResult(Result.failure(IllegalStateException("Play Billing failed: ${result.debugMessage}")))
            return
        }
        purchases.forEach { purchase ->
            if (purchase.purchaseState != Purchase.PurchaseState.PURCHASED) return@forEach
            val paymentId = purchase.accountIdentifiers?.obfuscatedAccountId ?: return@forEach
            val pending = pendingByPaymentId[paymentId] ?: return@forEach
            if (!purchase.products.contains(pending.productId)) {
                onResult(Result.failure(IllegalStateException("Google Play product does not match Payment Intent")))
                return@forEach
            }
            scope.launch {
                runCatching {
                    val receipt = verifyWithFabushiPay(pending.verifyPath, purchase.purchaseToken)
                    check(receipt.paymentId == paymentId && receipt.status == "succeeded") {
                        "Fabushi Pay did not confirm the Google Play purchase"
                    }
                    finalizeGooglePurchase(purchase, pending.consumable)
                    pendingByPaymentId.remove(paymentId)
                    receipt
                }.also(onResult)
            }
        }
    }

    fun close() {
        pendingByPaymentId.clear()
        billingClient.endConnection()
    }

    private suspend fun queryProduct(productId: String, productType: String): ProductDetails =
        suspendCancellableCoroutine { continuation ->
            val product = QueryProductDetailsParams.Product.newBuilder()
                .setProductId(productId)
                .setProductType(productType)
                .build()
            val params = QueryProductDetailsParams.newBuilder()
                .setProductList(listOf(product))
                .build()
            billingClient.queryProductDetailsAsync(params) { result, queryResult ->
                if (result.responseCode != BillingClient.BillingResponseCode.OK) {
                    continuation.resumeWithException(
                        IllegalStateException("Unable to query Play product: ${result.debugMessage}"),
                    )
                    return@queryProductDetailsAsync
                }
                val details = queryResult.productDetailsList.firstOrNull()
                if (details == null) {
                    continuation.resumeWithException(IllegalStateException("Google Play product is unavailable"))
                } else {
                    continuation.resume(details)
                }
            }
        }

    private suspend fun verifyWithFabushiPay(path: String, purchaseToken: String): FabushiPayReceipt {
        val token = accessTokenProvider()
        val endpoint = URL(URL(serviceBaseUrl), path)
        val connection = (endpoint.openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            connectTimeout = 15_000
            readTimeout = 20_000
            doOutput = true
            setRequestProperty("Authorization", "Bearer $token")
            setRequestProperty("Content-Type", "application/json")
        }
        try {
            connection.outputStream.use { output ->
                output.write(JSONObject().put("purchaseToken", purchaseToken).toString().toByteArray())
            }
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val body = stream?.bufferedReader()?.use { it.readText() }.orEmpty()
            check(status in 200..299) { "Fabushi Pay verification failed ($status): $body" }
            val json = JSONObject(body)
            return FabushiPayReceipt(
                paymentId = json.getString("paymentId"),
                sku = json.getString("sku"),
                rail = json.getString("rail"),
                amount = json.getLong("amount"),
                currency = json.getString("currency"),
                status = json.getString("status"),
            )
        } finally {
            connection.disconnect()
        }
    }

    private fun finalizeGooglePurchase(purchase: Purchase, consumable: Boolean) {
        if (consumable) {
            billingClient.consumeAsync(
                ConsumeParams.newBuilder().setPurchaseToken(purchase.purchaseToken).build(),
            ) { _, _ -> }
        } else if (!purchase.isAcknowledged) {
            billingClient.acknowledgePurchase(
                AcknowledgePurchaseParams.newBuilder()
                    .setPurchaseToken(purchase.purchaseToken)
                    .build(),
            ) { }
        }
    }
}
