package io.swps.nockster

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.ActivityNotFoundException
import android.graphics.Bitmap
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import android.Manifest
import android.content.pm.PackageManager
import android.net.Uri
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.journeyapps.barcodescanner.BarcodeView
import com.journeyapps.barcodescanner.BarcodeCallback
import com.journeyapps.barcodescanner.BarcodeResult
import com.journeyapps.barcodescanner.DefaultDecoderFactory
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.Image
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter
import kotlinx.coroutines.launch
import kotlinx.coroutines.CoroutineScope
import org.json.JSONArray
import org.json.JSONObject
import java.text.DateFormat
import java.util.Date

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NocksterApp(model: WalletModel) {
    val appScope = rememberCoroutineScope()
    val focus = LocalFocusManager.current
    LaunchedEffect(model.privateScreen) { if (model.privateScreen) focus.clearFocus() }
    val scheme = if (isSystemInDarkTheme()) NocksterDarkColors else NocksterLightColors
    MaterialTheme(colorScheme = scheme, shapes = Shapes(small = RoundedCornerShape(8.dp), medium = RoundedCornerShape(12.dp), large = RoundedCornerShape(16.dp))) {
        Surface(Modifier.fillMaxSize()) {
            // Entry forms keep their in-memory drafts; FLAG_SECURE protects the background window.
            if (!model.ready) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            } else if (model.snapshot.optBoolean("exists") && !model.unlocked) {
                UnlockScreen(model)
            } else if (!model.snapshot.optBoolean("exists") || model.wallets.isEmpty()) {
                SetupScreen(model, appScope) {}
            } else key(model.session) {
                var tab by remember { mutableIntStateOf(0) }
                var addingWallet by remember { mutableStateOf(false) }
                var addressBook by remember { mutableStateOf(false) }
                LaunchedEffect(model.pendingRecipients, model.unlocked) {
                    if (model.pendingRecipients.isNotEmpty() && model.unlocked) { addressBook = false; tab = 1 }
                }
                BackHandler(model.unlocked && (tab != 0 || addingWallet || addressBook)) { addingWallet = false; addressBook = false; tab = 0 }
                if (addingWallet) {
                    SetupScreen(model, appScope) { addingWallet = false }
                } else {
                    val labels = listOf("Wallet", "Send", "Receive", "Activity", "Settings")
                    val icons = listOf(Icons.Default.AccountBalanceWallet, Icons.AutoMirrored.Filled.Send, Icons.Default.QrCode, Icons.Default.History, Icons.Default.Settings)
                    Scaffold(
                        topBar = { TopAppBar(title = { if (addressBook) Text("Address book") else if (tab == 0) NocksterBrand() else Text(labels[tab], fontWeight = FontWeight.SemiBold) },
                            actions = { IconButton(onClick = { model.lock() }) { Icon(Icons.Default.Lock, "Lock wallet") } }) },
                        bottomBar = {
                            NavigationBar(containerColor = MaterialTheme.colorScheme.background, tonalElevation = 0.dp) { labels.forEachIndexed { index, label ->
                                NavigationBarItem(selected = tab == index, onClick = { addressBook = false; tab = index }, icon = { Icon(icons[index], label) }, label = { Text(label) })
                            } }
                        }
                    ) { padding ->
                        val content: @Composable () -> Unit = {
                        Column(Modifier.fillMaxSize().padding(padding).imePadding().verticalScroll(rememberScrollState()).padding(20.dp),
                            verticalArrangement = Arrangement.spacedBy(16.dp)) {
                            if (model.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                            if (addressBook) {
                                TextButton(onClick = { addressBook = false }) { Text("Back to wallet") }
                                AddressBookScreen(model)
                            } else when (tab) {
                                0 -> AccountScreen(model, add = { addingWallet = true }, openAddressBook = { addressBook = true })
                                1 -> SendScreen(model)
                                2 -> ReceiveScreen(model)
                                3 -> ActivityScreen(model)
                                4 -> SettingsScreen(model)
                            }
                        }
                        }
                        if (tab == 3 && !addressBook) PullToRefreshBox(isRefreshing = model.busy, onRefresh = { appScope.launch { model.perform("refresh") } }) { content() }
                        else content()
                    }
                }
            }
            if (!model.privateScreen) (model.error ?: model.notice)?.let { message ->
                AlertDialog(onDismissRequest = { model.error = null; model.notice = null }, title = { Text("Nockster") }, text = { Text(message) },
                    confirmButton = { TextButton(onClick = { model.error = null; model.notice = null }) { Text("OK") } })
            }
        }
    }
}

@Composable
private fun UnlockScreen(model: WalletModel) {
    var password by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    val focus = LocalFocusManager.current
    val unlock = { focus.clearFocus(); val value = password; password = ""; scope.launch { model.perform("unlock", JSONObject().put("password", value)) }; Unit }
    Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding().verticalScroll(rememberScrollState()).padding(28.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(24.dp, Alignment.CenterVertically)) {
        NocksterBrand(large = true)
        Text("Nockchain wallet", style = MaterialTheme.typography.titleMedium)
        OutlinedTextField(password, { password = it }, label = { Text("Wallet password") },
            visualTransformation = PasswordVisualTransformation(), singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { if (password.isNotEmpty() && !model.busy) unlock() }), modifier = Modifier.fillMaxWidth())
        Button(onClick = unlock, shape = RoundedCornerShape(12.dp),
            enabled = password.isNotEmpty() && !model.busy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text("Unlock Wallet") }
        if (model.deviceUnlockEnabled) OutlinedButton(onClick = { scope.launch { model.unlockWithDevice() } }, enabled = !model.busy) { Text("Use PIN or Biometrics") }
        if (model.busy) CircularProgressIndicator()
    }
}

@Composable
private fun SetupScreen(model: WalletModel, appScope: CoroutineScope, done: () -> Unit) {
    val scope = rememberCoroutineScope()
    var importing by remember { mutableStateOf(false) }
    var name by remember { mutableStateOf(model.snapshot.text("suggestedWalletName")) }
    var secret by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var confirmation by remember { mutableStateOf("") }
    var enableDeviceUnlock by remember { mutableStateOf(false) }
    var mnemonic by remember { mutableStateOf<List<String>>(emptyList()) }
    var first by remember { mutableStateOf("") }
    var last by remember { mutableStateOf("") }
    var saved by remember { mutableStateOf(false) }
    val nameExists = model.wallets.any { it.text("name") == name.trim() }
    val valid = name.isNotBlank() && !nameExists && (if (importing) secret.isNotBlank() else mnemonic.isNotEmpty() && saved && first.trim() == mnemonic.first() && last.trim() == mnemonic.last()) &&
        (model.snapshot.optBoolean("exists") || password.length >= 12 && password == confirmation)
    Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        NocksterBrand()
        Text(if (model.wallets.isEmpty()) "A Nockchain mobile wallet just for you." else "Add a wallet", style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
        Text("Create a Nockchain wallet or bring your existing keys.")
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            FilterChip(selected = !importing, onClick = { importing = false }, label = { Text("Create") })
            FilterChip(selected = importing, onClick = { importing = true }, label = { Text("Import") })
        }
        OutlinedTextField(name, { name = it }, label = { Text("Wallet name") }, isError = nameExists,
            textStyle = LocalTextStyle.current.copy(color = if (nameExists) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface),
            supportingText = { if (nameExists) Text("A wallet with this name already exists") }, modifier = Modifier.fillMaxWidth())
        if (importing) {
            OutlinedTextField(secret, { secret = it }, label = { Text("Recovery phrase or extended private key") },
                keyboardOptions = KeyboardOptions(autoCorrectEnabled = false), minLines = 4, modifier = Modifier.fillMaxWidth())
        } else if (mnemonic.isEmpty()) {
            Button(onClick = { scope.launch {
                val words = model.perform("generate")?.optJSONArray("mnemonic")
                if (words != null) mnemonic = (0 until words.length()).map { words.getString(it) }
            } }, enabled = !model.busy) { Text("Generate Recovery Phrase") }
        } else {
            Text("Secret Recovery Phrase", style = MaterialTheme.typography.titleLarge)
            mnemonic.chunked(2).forEachIndexed { index, pair ->
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    pair.forEachIndexed { column, word -> Text("${index * 2 + column + 1}. $word", Modifier.weight(1f)) }
                }
            }
            Text("Write these words down in order. Anyone with them can spend your funds.")
            Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(saved, { saved = it }); Text("I wrote down all 24 words") }
            OutlinedTextField(first, { first = it }, label = { Text("First word") }, keyboardOptions = KeyboardOptions(autoCorrectEnabled = false))
            OutlinedTextField(last, { last = it }, label = { Text("Last word") }, keyboardOptions = KeyboardOptions(autoCorrectEnabled = false))
        }
        if (!model.snapshot.optBoolean("exists")) {
            Text("Protect this device", style = MaterialTheme.typography.titleLarge)
            Text("Use at least 12 characters. Numbers, symbols, and uppercase letters are optional. Try a few unrelated words.", style = MaterialTheme.typography.bodySmall)
            OutlinedTextField(password, { password = it }, label = { Text("Wallet password") },
                supportingText = { Text(if (password.length >= 12) "Minimum length met" else "${password.length}/12 characters") },
                visualTransformation = PasswordVisualTransformation(), singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false), modifier = Modifier.fillMaxWidth())
            OutlinedTextField(confirmation, { confirmation = it }, label = { Text("Confirm password") },
                isError = confirmation.isNotEmpty() && password != confirmation,
                supportingText = { if (confirmation.isNotEmpty()) Text(if (password == confirmation) "Passwords match" else "Passwords don’t match") },
                visualTransformation = PasswordVisualTransformation(), singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false), modifier = Modifier.fillMaxWidth())
            Text("Nockster locks when you leave the app. This password encrypts your wallet on this device.", style = MaterialTheme.typography.bodySmall)
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text("Enable device PIN or biometrics", Modifier.weight(1f))
                Switch(checked = enableDeviceUnlock, onCheckedChange = { enableDeviceUnlock = it }, enabled = !model.busy)
            }
            Text("Use this device’s screen-lock PIN, pattern, password, or biometrics to unlock. This is optional and can also be enabled in Settings. Your wallet password still works.", style = MaterialTheme.typography.bodySmall)
        }
        Button(onClick = {
            val key = if (importing) secret else mnemonic.joinToString(" ")
            val setupPassword = password
            val enrollDeviceUnlock = !model.snapshot.optBoolean("exists") && enableDeviceUnlock
            // Enrollment must survive SetupScreen leaving composition after the import.
            appScope.launch {
                if (model.perform("import", JSONObject().put("name", name).put("key", key).put("password", setupPassword)) != null) {
                    secret = ""; password = ""; confirmation = ""; mnemonic = emptyList()
                    if (enrollDeviceUnlock) model.enableDeviceUnlock(setupPassword)
                    done()
                }
            }
        }, enabled = valid && !model.busy, modifier = Modifier.fillMaxWidth()) { Text(if (importing) "Import Wallet" else "Create Wallet") }
        if (model.wallets.isNotEmpty()) TextButton(onClick = done) { Text("Cancel") }
        if (model.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
    }
}

private fun shortWalletAddress(address: String): String = if (address.length > 12) "${address.take(6)}…${address.takeLast(4)}" else address

private fun groupedAmount(value: String): String {
    val parts = value.split('.', limit = 2)
    return parts[0].replace(Regex("(?<=\\d)(?=(\\d{3})+$)"), ",") + if (parts.size > 1) ".${parts[1]}" else ""
}

@Composable
private fun WalletExplorerLink(address: String) {
    val context = LocalContext.current
    TextButton(onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://nockblocks.com/address/$address"))) }, enabled = address.isNotEmpty(),
        colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onSurfaceVariant)) {
        Text("Nockblocks", style = MaterialTheme.typography.labelSmall)
        Spacer(Modifier.width(4.dp))
        Icon(Icons.Default.OpenInNew, "View address on Nockblocks", Modifier.size(16.dp))
    }
}

@Composable
private fun WalletAddressActions(address: String) {
    val context = LocalContext.current
    var copied by remember(address) { mutableStateOf(false) }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        TextButton(onClick = {
            (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Nockchain address", address))
            copied = true
        }, colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onSurfaceVariant)) {
            Icon(Icons.Default.ContentCopy, null, Modifier.size(16.dp)); Spacer(Modifier.width(6.dp))
            Text(if (copied) "Copied" else "Copy address", style = MaterialTheme.typography.labelSmall)
        }
        WalletExplorerLink(address)
    }
}

@Composable
private fun ManageWalletDialog(model: WalletModel, wallet: JSONObject, dismiss: () -> Unit) {
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf(wallet.text("name")) }
    var deleting by remember { mutableStateOf(false) }
    var confirmation by remember { mutableStateOf("") }
    var backupConfirmed by remember { mutableStateOf(false) }
    val nameExists = model.wallets.any { it.text("id") != wallet.text("id") && it.text("name") == name.trim() }
    AlertDialog(onDismissRequest = { if (!model.busy) dismiss() },
        title = { Text(if (deleting) "Delete ${wallet.text("name")}?" else "Manage Wallet") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (deleting) {
                    Text("Deleting removes this wallet from this device. You need its recovery phrase or extended private key to restore access to its funds.")
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(backupConfirmed, { backupConfirmed = it }, enabled = !model.busy)
                        Text("I have backed up this wallet’s recovery phrase or extended private key")
                    }
                    Text("Type “${wallet.text("name")}” to confirm deletion.")
                    OutlinedTextField(confirmation, { confirmation = it }, label = { Text("Wallet name") }, enabled = !model.busy,
                        keyboardOptions = KeyboardOptions(autoCorrectEnabled = false), singleLine = true)
                } else {
                    OutlinedTextField(name, { name = it }, label = { Text("Wallet name") }, enabled = !model.busy, singleLine = true, isError = nameExists,
                        textStyle = LocalTextStyle.current.copy(color = if (nameExists) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface),
                        supportingText = { if (nameExists) Text("A wallet with this name already exists") })
                    Text(wallet.text("address"), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                    WalletAddressActions(wallet.text("address"))
                    if (model.wallets.size > 1) {
                        TextButton(onClick = { deleting = true }, enabled = !model.busy, colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error)) { Text("Delete Wallet") }
                    } else Text("Add another wallet before deleting your only wallet.")
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { scope.launch {
                val result = if (deleting) model.perform("deleteWallet", JSONObject().put("walletId", wallet.text("id")).put("confirmation", confirmation).put("backupConfirmed", backupConfirmed))
                    else model.perform("rename", JSONObject().put("walletId", wallet.text("id")).put("name", name))
                if (result != null) dismiss()
            } }, enabled = !model.busy && if (deleting) backupConfirmed && confirmation == wallet.text("name") else name.isNotBlank() && !nameExists) {
                Text(if (deleting) "Delete Wallet" else "Save name", color = if (deleting) MaterialTheme.colorScheme.error else Color.Unspecified)
            }
        },
        dismissButton = { TextButton(onClick = dismiss, enabled = !model.busy) { Text("Cancel") } })
}

@Composable
private fun AccountScreen(model: WalletModel, add: () -> Unit, openAddressBook: () -> Unit) {
    val scope = rememberCoroutineScope()
    var editingWallet by remember { mutableStateOf<JSONObject?>(null) }
    editingWallet?.let { wallet -> ManageWalletDialog(model, wallet) { editingWallet = null } }
    LaunchedEffect(Unit) { model.perform("refresh") }
    var showNicks by remember(model.snapshot.text("activeId")) { mutableStateOf(false) }
    val balanceColor = if (showNicks) {
        if (isSystemInDarkTheme()) Color(0xFF5EEAD4) else Color(0xFF0F766E)
    } else MaterialTheme.colorScheme.onSurface
    Card(onClick = { showNicks = !showNicks }, enabled = model.active?.isNull("balance") == false,
        modifier = Modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer), shape = RoundedCornerShape(16.dp)) {
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(model.active?.text("name") ?: "Wallet", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f, fill = false))
                Text(shortWalletAddress(model.active?.text("address") ?: ""), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
            }
            Text("Available balance", color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(model.active?.text(if (showNicks) "balanceNicks" else "balance", "—") ?: "—",
                style = MaterialTheme.typography.displaySmall, fontWeight = FontWeight.SemiBold, letterSpacing = (-1).sp, color = balanceColor)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(if (showNicks) "NICKS" else "NOCK", style = MaterialTheme.typography.titleMedium, color = balanceColor)
                Icon(Icons.Default.SwapHoriz, if (showNicks) "Show balance in NOCK" else "Show balance in nicks", Modifier.size(18.dp), tint = balanceColor)
            }
            UsdSubtitle(model.active?.text("balanceUsd"))
        }
    }
    model.snapshot.text("networkError").takeIf { it.isNotEmpty() }?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    Text("Your wallets", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
    model.wallets.forEach { wallet ->
        OutlinedCard(onClick = { scope.launch { if (model.perform("select", JSONObject().put("walletId", wallet.text("id"))) != null) model.perform("refresh") } },
            enabled = !model.busy, modifier = Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(wallet.text("name"), modifier = Modifier.weight(1f, fill = false), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(shortWalletAddress(wallet.text("address")), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                    Spacer(Modifier.weight(1f))
                    if (wallet.text("id") == model.snapshot.text("activeId")) Icon(Icons.Default.CheckCircle, "Selected", Modifier.size(20.dp))
                    IconButton(onClick = { editingWallet = wallet }, enabled = !model.busy) { Icon(Icons.Default.MoreVert, "Manage ${wallet.text("name")}") }
                }
                Text("${wallet.text("balance", "—")} NOCK", style = MaterialTheme.typography.bodyMedium)
                WalletAddressActions(wallet.text("address"))
            }
        }
    }
    OutlinedButton(onClick = openAddressBook, modifier = Modifier.fillMaxWidth()) { Icon(Icons.Default.Contacts, null); Spacer(Modifier.width(8.dp)); Text("Address book") }
    Button(onClick = add, enabled = !model.busy) { Icon(Icons.Default.Add, null); Text("Add Wallet") }
    OutlinedButton(onClick = { scope.launch { model.perform("refresh") } }, enabled = !model.busy) { Text("Refresh") }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SendScreen(model: WalletModel) {
    val scope = rememberCoroutineScope()
    var recipients by remember { mutableStateOf(listOf("" to "")) }
    var bridgeMode by remember { mutableStateOf(false) }
    var bridgeDestination by remember { mutableStateOf("") }
    var bridgeAmount by remember { mutableStateOf("") }
    var privateOutputs by remember { mutableStateOf(false) }
    var preview by remember { mutableStateOf<JSONObject?>(null) }
    var submitted by remember { mutableStateOf<String?>(null) }
    var inNicks by remember { mutableStateOf(false) }
    var contactFilter by remember { mutableStateOf("") }
    var contactRecipient by remember { mutableStateOf<Int?>(null) }
    var editingRecipient by remember { mutableStateOf<Int?>(null) }
    var scanningRecipient by remember { mutableStateOf<Int?>(null) }
    scanningRecipient?.let { index ->
        ScanAddressDialog(model, onDismiss = { scanningRecipient = null }) { value ->
            scanningRecipient = null
            scope.launch {
                val address = model.perform("scanAddress", JSONObject().put("key", value))?.text("address")
                if (address != null && index in recipients.indices) {
                    recipients = recipients.mapIndexed { i, pair -> if (i == index) address to pair.second else pair }
                    editingRecipient = null
                }
            }
        }
    }
    val context = LocalContext.current
    LaunchedEffect(model.pendingRecipients) {
        if (model.pendingRecipients.isNotEmpty()) { bridgeMode = false; inNicks = false; recipients = model.pendingRecipients; model.pendingRecipients = emptyList(); preview = null }
    }
    Text("From ${model.active?.text("name") ?: "Wallet"}", style = MaterialTheme.typography.titleMedium)
    TextButton(onClick = { bridgeDestination = ""; bridgeAmount = ""; recipients = listOf("" to ""); privateOutputs = false; inNicks = false; preview = null; editingRecipient = null; contactRecipient = null }, enabled = !model.busy) { Text("Reset fields") }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(selected = !bridgeMode, onClick = { bridgeMode = false }, enabled = !model.busy, label = { Text("Send NOCK") })
        FilterChip(selected = bridgeMode, onClick = { bridgeMode = true }, enabled = !model.busy, label = { Text("Bridge to Base") })
    }
    if (bridgeMode) {
        OutlinedTextField(bridgeDestination, { bridgeDestination = it }, label = { Text("Receiving Base address (0x…)") }, enabled = !model.busy, keyboardOptions = KeyboardOptions(autoCorrectEnabled = false), modifier = Modifier.fillMaxWidth())
        OutlinedTextField(bridgeAmount, { bridgeAmount = it }, label = { Text("Amount in NOCK") }, enabled = !model.busy, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal), modifier = Modifier.fillMaxWidth())
        UsdSubtitle(composeUsd(bridgeAmount, false, model.snapshot.optDouble("usdPerNock")))
        Text("Minimum 100,000 NOCK. Receive wrapped NOCK on Base after at least 400 Nockchain confirmations and bridge processing. The protocol deducts approximately 0.3% from the payout. Verify that you control the Base address; deposits cannot be reversed.")
    } else {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        listOf(false to "NOCK", true to "NICKS").forEach { (nicks, label) ->
            FilterChip(selected = inNicks == nicks, enabled = !model.busy, onClick = {
                if (inNicks != nicks) scope.launch {
                    val inputs = JSONArray(recipients.map { JSONObject().put("amount", it.second) })
                    val values = model.perform("convertAmounts", JSONObject().put("amountUnit", if (inNicks) "nicks" else "nock").put("recipients", inputs))?.optJSONArray("amounts")
                    if (values != null) { recipients = recipients.mapIndexed { i, pair -> pair.first to values.getString(i) }; inNicks = nicks; preview = null }
                }
            }, label = { Text(label) })
        }
    }
    recipients.forEachIndexed { index, recipient ->
        Text("Recipient ${index + 1}")
        val contact = model.snapshot.optJSONArray("contacts")?.objects()?.find { it.text("address") == recipient.first.trim() }
        contact?.let {
            Text(it.text("alias"), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        }
        if (contact != null && editingRecipient != index) {
            OutlinedCard(onClick = { editingRecipient = index }, enabled = !model.busy, modifier = Modifier.fillMaxWidth()) {
                ListItem(headlineContent = { Text("${recipient.first.take(8)}…${recipient.first.takeLast(8)}", fontFamily = FontFamily.Monospace) }, trailingContent = { Icon(Icons.Default.Edit, "Edit address") })
            }
        } else OutlinedTextField(recipient.first, { value -> recipients = recipients.mapIndexed { i, pair -> if (i == index) value to pair.second else pair } },
            label = { Text("Nockchain address") }, enabled = !model.busy, keyboardOptions = KeyboardOptions(autoCorrectEnabled = false), modifier = Modifier.fillMaxWidth())
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            TextButton(onClick = {
                val value = (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip?.getItemAt(0)?.coerceToText(context)?.toString()?.trim().orEmpty()
                if (value.isNotEmpty()) recipients = recipients.mapIndexed { i, pair -> if (i == index) value to pair.second else pair }
            }, enabled = !model.busy) { Icon(Icons.Default.ContentPaste, null); Spacer(Modifier.width(6.dp)); Text("Paste") }
            TextButton(onClick = { contactFilter = ""; contactRecipient = index; scope.launch { model.perform("contacts") } }, enabled = !model.busy) { Icon(Icons.Default.Contacts, null); Spacer(Modifier.width(6.dp)); Text("Address book") }
            IconButton(onClick = { scanningRecipient = index }, enabled = !model.busy) { Icon(Icons.Default.QrCodeScanner, "Scan QR code") }
            IconButton(onClick = { (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Nockchain address", recipient.first.trim())) }, enabled = !model.busy && recipient.first.isNotBlank()) { Icon(Icons.Default.ContentCopy, "Copy address") }
        }
        if (index == recipients.lastIndex) TextButton(onClick = { recipients = recipients + ("" to "") }, enabled = recipients.size < 16 && !model.busy) { Text("Add Recipient") }
        OutlinedTextField(recipient.second, { value -> recipients = recipients.mapIndexed { i, pair -> if (i == index) pair.first to value else pair } },
            label = { Text(if (inNicks) "Amount in nicks" else "Amount in NOCK") }, enabled = !model.busy, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = if (inNicks) KeyboardType.Number else KeyboardType.Decimal), modifier = Modifier.fillMaxWidth())
        UsdSubtitle(composeUsd(recipient.second, inNicks, model.snapshot.optDouble("usdPerNock")))
        if (recipients.size > 1) TextButton(onClick = { recipients = recipients.filterIndexed { i, _ -> i != index }; editingRecipient = null }, enabled = !model.busy) { Text("Remove") }
    }
    Row(verticalAlignment = Alignment.CenterVertically) { Switch(privateOutputs, { privateOutputs = it }); Text("Private outputs", Modifier.padding(start = 12.dp)) }
    }
    Button(onClick = { scope.launch {
        val inputs = JSONArray(recipients.map { JSONObject().put("address", it.first).put("amount", it.second) })
        val fields = if (bridgeMode) JSONObject().put("bridge", JSONObject().put("destination", bridgeDestination).put("amount", bridgeAmount))
            else JSONObject().put("recipients", inputs).put("amountUnit", if (inNicks) "nicks" else "nock").put("privateOutputs", privateOutputs)
        if (bridgeMode) inNicks = false
        preview = model.perform("prepare", fields)?.optJSONObject("preview")
    } }, enabled = !model.busy && (if (bridgeMode) bridgeDestination.isNotBlank() && bridgeAmount.isNotBlank() else recipients.all { it.first.isNotBlank() && it.second.isNotBlank() }), modifier = Modifier.fillMaxWidth()) { Text(if (bridgeMode) "Review Bridge" else "Review Payment") }
    contactRecipient?.let { index ->
        ModalBottomSheet(onDismissRequest = { contactRecipient = null }) {
            Column(Modifier.padding(24.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Choose an address", style = MaterialTheme.typography.headlineSmall)
                OutlinedTextField(contactFilter, { contactFilter = it }, label = { Text("Filter by nickname or address") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                val contacts = model.snapshot.optJSONArray("contacts")?.objects().orEmpty()
                val filtered = contacts.filter { it.text("alias").contains(contactFilter.trim(), ignoreCase = true) || it.text("address").contains(contactFilter.trim(), ignoreCase = true) }
                if (contacts.isNotEmpty() && filtered.isEmpty()) Text("No matching addresses")
                if (contacts.isEmpty()) Text(if (model.busy) "Loading addresses…" else "Your address book is empty. Add addresses from the Wallet page.")
                filtered.forEach { contact ->
                    OutlinedCard(onClick = {
                        recipients = recipients.mapIndexed { i, pair -> if (i == index) contact.text("address") to pair.second else pair }
                        editingRecipient = null
                        contactRecipient = null
                    }, modifier = Modifier.fillMaxWidth()) {
                        ListItem(headlineContent = { Text(contact.text("alias")) }, supportingContent = { Text(contact.text("address"), style = MaterialTheme.typography.bodySmall) })
                    }
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
    preview?.let { payment ->
        ModalBottomSheet(onDismissRequest = { if (!model.busy) preview = null }, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
            Column(Modifier.padding(24.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                val bridge = payment.optJSONObject("bridge")
                Text(if (bridge != null) "Review Bridge" else "Review Payment", style = MaterialTheme.typography.headlineMedium)
                if (bridge != null) {
                    Text("Receive on Base")
                    SelectionContainer { Text(bridge.text("destination"), fontFamily = FontFamily.Monospace) }
                    Text("Protocol fee (≈0.3%): ${bridge.text("protocolFee")} NOCK")
                    Text("Expected on Base: ${bridge.text("expectedReceived")} NOCK")
                    Text("The protocol fee is deducted from the deposit. Wait at least 400 Nockchain confirmations plus bridge processing. Check the complete receiving address before confirming.")
                } else payment.getJSONArray("recipients").objects().forEach { recipient ->
                    model.snapshot.optJSONArray("contacts")?.objects()?.find { it.text("address") == recipient.text("address") }?.let {
                        Text(it.text("alias"), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                    }
                    Text(recipient.text("address"))
                    Text("${recipient.text(if (inNicks) "amountNicks" else "amount")} ${if (inNicks) "NICKS" else "NOCK"}")
                    UsdSubtitle(recipient.text("amountUsd"))
                }
                HorizontalDivider()
                Text("Net sent: ${payment.text(if (inNicks) "netSentNicks" else "netSent")} ${if (inNicks) "NICKS" else "NOCK"}")
                UsdSubtitle(payment.text("netSentUsd"))
                Text("Network fee: ${payment.text(if (inNicks) "feeNicks" else "fee")} ${if (inNicks) "NICKS" else "NOCK"}")
                UsdSubtitle(payment.text("feeUsd"))
                Text("Total: ${payment.text(if (inNicks) "totalNicks" else "total")} ${if (inNicks) "NICKS" else "NOCK"}", style = MaterialTheme.typography.titleLarge)
                UsdSubtitle(payment.text("totalUsd"))
                Text("Private outputs: ${if (payment.optBoolean("privateOutputs")) "On" else "Off"}")
                Text("This sends funds on Nockchain mainnet. Check every address and amount.")
                PaymentAuthentication(model) { password -> scope.launch {
                    val reply = model.confirmSubmission("send", JSONObject().put("previewId", payment.text("id")), password)
                    reply?.text("txId")?.let { preview = null; submitted = it; bridgeAmount = ""; recipients = listOf("" to "") }
                } }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
    submitted?.let { tx -> AlertDialog(onDismissRequest = { submitted = null }, title = { Text("Transaction Submitted") },
        text = { Text("Track confirmation in Activity.\n$tx") }, confirmButton = { TextButton(onClick = { submitted = null }) { Text("Done") } }) }
}

@Composable
private fun ReceiveScreen(model: WalletModel) {
    val context = LocalContext.current
    val address = model.active?.text("address") ?: ""
    val qr = remember(address) {
        if (address.isEmpty()) null else QRCodeWriter().encode(address, BarcodeFormat.QR_CODE, 640, 640).let { matrix ->
            Bitmap.createBitmap(640, 640, Bitmap.Config.ARGB_8888).apply {
                setPixels(IntArray(640 * 640) { if (matrix[it % 640, it / 640]) android.graphics.Color.BLACK else android.graphics.Color.WHITE }, 0, 640, 0, 0, 640, 640)
            }.asImageBitmap()
        }
    }
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(24.dp)) {
        Text("Receive NOCK", style = MaterialTheme.typography.headlineMedium)
        qr?.let { Image(it, "QR code for your Nockchain address", Modifier.sizeIn(maxWidth = 300.dp, maxHeight = 300.dp).fillMaxWidth().aspectRatio(1f)) }
        Text(address, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodyMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            OutlinedButton(onClick = { (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Nockchain address", address)) }) { Text("Copy") }
            Button(onClick = { context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, address), "Share address")) }) { Text("Share") }
        }
        Text("This is a layer 1 Nockchain address", style = MaterialTheme.typography.bodySmall)
        WalletExplorerLink(address)
    }
}

@Composable
private fun PaymentAuthentication(model: WalletModel, submit: (String?) -> Unit) {
    var password by remember { mutableStateOf("") }
    val focus = LocalFocusManager.current
    val confirm = {
        if (password.isNotEmpty() && !model.busy) {
            val credential = password
            password = ""
            focus.clearFocus()
            submit(credential)
        }
    }
    OutlinedTextField(password, { password = it }, label = { Text("Wallet password") },
        singleLine = true, enabled = !model.busy, visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { confirm() }), modifier = Modifier.fillMaxWidth())
    Button(onClick = { confirm() }, enabled = password.isNotEmpty() && !model.busy, modifier = Modifier.fillMaxWidth()) { Text("Confirm and Send") }
    if (model.deviceUnlockEnabled) OutlinedButton(onClick = { password = ""; focus.clearFocus(); submit(null) }, enabled = !model.busy, modifier = Modifier.fillMaxWidth()) { Text("Confirm with device unlock") }
    model.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ActivityScreen(model: WalletModel) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var selectedId by remember(model.snapshot.text("activeId")) { mutableStateOf<String?>(null) }
    LaunchedEffect(model.snapshot.text("activeId")) {
        while (true) {
            if (!model.busy) model.perform("refresh")
            kotlinx.coroutines.delay(30_000)
        }
    }
    val selected = model.history.find { it.text("id") == selectedId }
    selected?.let { transaction ->
        ModalBottomSheet(onDismissRequest = { if (!model.busy) selectedId = null }, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
            Column(Modifier.padding(24.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text(if (transaction.text("direction") == "sent") "Payment sent" else "Payment received", style = MaterialTheme.typography.headlineMedium)
                Text("${groupedAmount(transaction.text("amount"))} NOCK", style = MaterialTheme.typography.headlineLarge)
                UsdSubtitle(transaction.text("amountUsd"))
                Text("Status: ${transaction.text("statusLabel", "Pending")}")
                Text(DateFormat.getDateTimeInstance().format(Date(transaction.optLong("timestamp"))))
                Text("Total fees: ${groupedAmount(transaction.text("fee"))} NOCK")
                UsdSubtitle(transaction.text("feeUsd"))
                if (!transaction.isNull("blockHeight")) Text("Block: ${groupedAmount(transaction.optLong("blockHeight").toString())}")
                Text(if (transaction.optJSONObject("bridge") != null) "To Base" else if (transaction.text("direction") == "self") "Own address" else if (transaction.text("direction") == "sent") "To" else "From", style = MaterialTheme.typography.titleMedium)
                transaction.optJSONArray("parties")?.objects().orEmpty().forEach { party ->
                    CopyAddress(party.text("label").ifEmpty { null }, party.text("address"))
                }
                transaction.optJSONObject("bridge")?.let { bridge ->
                    Text("Bridge to Base", style = MaterialTheme.typography.titleMedium)
                    CopyAddress("Base destination", bridge.text("destination"))
                    Text("Bridge deposit: ${groupedAmount(bridge.text("amount"))} NOCK")
                    Text("Bridge protocol fee (≈0.3%): ${groupedAmount(bridge.text("protocolFee"))} NOCK")
                    Text("Expected on Base: ${groupedAmount(bridge.text("expectedReceived"))} NOCK")
                    Text("L1 confirmation does not mean delivery on Base. The bridge waits 400 blocks before processing.")
                    TextButton(onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://basescan.org/address/${bridge.text("destination")}"))) }) { Text("View destination on Base") }
                }
                Text("Transaction ID", style = MaterialTheme.typography.titleMedium)
                CopyAddress(null, transaction.text("id"))
                TextButton(onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://nockblocks.com/tx/${transaction.text("id")}"))) }) { Text("View on Nockblocks") }
                transaction.text("error").takeIf { it.isNotEmpty() }?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                if (transaction.optBoolean("canClear")) {
                    Text(transaction.text("pendingDetail"), style = MaterialTheme.typography.bodySmall)
                    var clearing by remember(transaction.text("id")) { mutableStateOf(false) }
                    TextButton(onClick = { clearing = true }, enabled = !model.busy) { Text("Clear unconfirmed transaction", color = MaterialTheme.colorScheme.error) }
                    if (clearing) AlertDialog(
                        onDismissRequest = { clearing = false },
                        title = { Text("Clear unconfirmed transaction?") },
                        text = { Text("This releases its reserved funds locally. It does not cancel the transaction on the network; it could still confirm. Check the explorer before sending again.") },
                        dismissButton = { TextButton(onClick = { clearing = false }) { Text("Cancel") } },
                        confirmButton = { TextButton(enabled = !model.busy, onClick = { scope.launch {
                            val id = transaction.text("id")
                            if (model.perform("clearPending", JSONObject().put("walletId", model.snapshot.text("activeId")).put("txId", id).put("confirmation", id)) != null) {
                                clearing = false
                                selectedId = null
                            }
                        } }) { Text("Clear") } }
                    )
                    var retry by remember(transaction.text("id")) { mutableStateOf(false) }
                    TextButton(onClick = { retry = !retry }, enabled = !model.busy) { Text("Retry Same Transaction") }
                    if (retry) PaymentAuthentication(model) { password -> scope.launch {
                        if (model.confirmSubmission("retry", JSONObject().put("txId", transaction.text("id")), password) != null) retry = false
                    } }
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
    Text(model.active?.text("name") ?: "Wallet", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
    Text(shortWalletAddress(model.active?.text("address") ?: ""), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
    if (model.history.isEmpty()) Text("No transactions yet. Pull to refresh.")
    model.history.forEach { transaction ->
        val sent = transaction.text("direction") == "sent"
        val self = transaction.text("direction") == "self"
        val color = if (sent) MaterialTheme.colorScheme.onSurface else Color(0xFF087F73)
        val parties = transaction.optJSONArray("parties")?.objects().orEmpty()
        val party = parties.firstOrNull()
        val label = party?.text("label")?.ifEmpty { null } ?: party?.text("address")?.let { "${it.take(8)}…${it.takeLast(8)}" } ?: "Nockchain"
        OutlinedCard(onClick = { selectedId = transaction.text("id") }, modifier = Modifier.fillMaxWidth()) {
            Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(if (self) Icons.Default.SwapHoriz else if (sent) Icons.Default.NorthEast else Icons.Default.SouthWest, null, tint = color)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(if (transaction.optJSONObject("bridge") != null) "Bridge to Base" else if (self) "Self transfer" else if (sent) "Sent" else "Received", color = color, style = MaterialTheme.typography.labelMedium)
                    Text((if (transaction.optJSONObject("bridge") != null) "To Base: " else if (self) "Own: " else if (sent) "To: " else "From: ") + label + if (parties.size > 1) " +${parties.size - 1}" else "", maxLines = 1, overflow = TextOverflow.Ellipsis, fontWeight = FontWeight.SemiBold)
                    Text(transaction.text("statusLabel", "Pending"), style = MaterialTheme.typography.bodySmall)
                    Text(DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(transaction.optLong("timestamp"))), style = MaterialTheme.typography.bodySmall)
                }
                Column(horizontalAlignment = Alignment.End) {
                    Text("${if (self) "" else if (sent) "−" else "+"}${groupedAmount(transaction.text("amount"))}", color = color, fontWeight = FontWeight.SemiBold)
                    Text("NOCK", style = MaterialTheme.typography.labelSmall)
                    UsdSubtitle(transaction.text("amountUsd"))
                }
                if (party != null) IconButton(onClick = { (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Nockchain address", party.text("address"))) }, modifier = Modifier.size(32.dp)) { Icon(Icons.Default.ContentCopy, "Copy address", modifier = Modifier.size(18.dp)) }
            }
        }
    }
    Button(onClick = { scope.launch { model.perform("refresh") } }, enabled = !model.busy) { Text("Refresh") }
}

@Composable
private fun CopyAddress(label: String?, address: String) {
    val context = LocalContext.current
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            if (label != null) Text(label, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(if (address.length > 20) "${address.take(8)}…${address.takeLast(8)}" else address, fontFamily = FontFamily.Monospace)
        }
        IconButton(onClick = { (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Nockchain", address)) }) { Icon(Icons.Default.ContentCopy, "Copy $address") }
    }
}

@Composable
private fun SettingsScreen(model: WalletModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var name by remember(model.active?.text("id")) { mutableStateOf(model.active?.text("name") ?: "") }
    var password by remember { mutableStateOf("") }
    var feedback by remember { mutableStateOf(false) }
    if (feedback) FeedbackDialog(model) { feedback = false }
    val focus = LocalFocusManager.current
    val enableDeviceUnlock = {
        if (password.isNotEmpty() && !model.busy) {
            focus.clearFocus()
            val value = password
            password = ""
            scope.launch { model.enableDeviceUnlock(value) }
        }
        Unit
    }
    val nameExists = model.wallets.any { it.text("id") != model.active?.text("id") && it.text("name") == name.trim() }
    OutlinedTextField(name, { name = it }, label = { Text("Wallet name") }, isError = nameExists,
        textStyle = LocalTextStyle.current.copy(color = if (nameExists) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface),
        supportingText = { if (nameExists) Text("A wallet with this name already exists") }, modifier = Modifier.fillMaxWidth())
    Button(onClick = { scope.launch { model.perform("rename", JSONObject().put("name", name)) } }, enabled = name.isNotBlank() && !nameExists && !model.busy) { Text("Rename Wallet") }
    HorizontalDivider()
    Text("Device PIN or biometrics", style = MaterialTheme.typography.titleLarge)
    Text("Choose your screen lock in the system prompt to use your device PIN. Your wallet password still works.", style = MaterialTheme.typography.bodySmall)
    model.deviceUnlockWarning?.let {
        Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
    }
    if (model.deviceUnlockEnabled) {
        OutlinedButton(enabled = !model.busy, onClick = {
            try { model.deviceUnlock.disable(); model.deviceUnlockEnabled = false } catch (failure: Exception) { model.error = failure.message }
        }) { Text("Disable device unlock") }
    } else {
        Text("Enter your wallet password to enable unlock with this device’s screen-lock PIN, pattern, password, or biometrics.", style = MaterialTheme.typography.bodySmall)
        OutlinedTextField(password, { password = it }, label = { Text("Confirm wallet password") },
            enabled = !model.busy, visualTransformation = PasswordVisualTransformation(), singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { if (password.isNotEmpty() && !model.busy) enableDeviceUnlock() }), modifier = Modifier.fillMaxWidth())
        Button(onClick = enableDeviceUnlock, enabled = password.isNotEmpty() && !model.busy) { Text("Continue to device verification") }
    }
    HorizontalDivider()
    Text("Privacy", style = MaterialTheme.typography.titleLarge)
    TextButton(onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://nockster.com/wallet/privacy"))) }) { Text("Privacy Policy") }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("Allow screenshots", Modifier.weight(1f))
        Switch(checked = model.screenCapture.allowed, onCheckedChange = { model.screenCapture.updateAllowed(it) },
            modifier = Modifier.semantics { contentDescription = "Allow screenshots" })
    }
    Text("Turn off to block screenshots and screen recording. Your wallet stays hidden in the app switcher.", style = MaterialTheme.typography.bodySmall)
    HorizontalDivider()
    OutlinedButton(onClick = { feedback = true }, modifier = Modifier.fillMaxWidth()) { Text("Send feedback") }
    Button(onClick = { model.lock() }, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
        shape = RoundedCornerShape(12.dp), colors = ButtonDefaults.buttonColors(containerColor = Color.Black, contentColor = Color.White)) {
        Icon(Icons.Default.Lock, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Lock wallet")
    }
    TextButton(onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://swps.io"))) }, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(vertical = 24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Developed by", color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
            Image(painterResource(R.drawable.swps_logo), "South Western Pool Supply", Modifier.width(240.dp),
                colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(MaterialTheme.colorScheme.onSurface))
        }
    }

}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun FeedbackDialog(model: WalletModel, onDismiss: () -> Unit) {
    val context = LocalContext.current
    var emailUnavailable by remember { mutableStateOf(false) }
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize()) {
            Scaffold(topBar = {
                TopAppBar(title = { Text("Send feedback") }, navigationIcon = {
                    IconButton(onClick = onDismiss) { Icon(Icons.Default.Close, "Close feedback") }
                })
            }) { padding ->
                Column(Modifier.fillMaxSize().padding(padding).imePadding().verticalScroll(rememberScrollState()).padding(20.dp),
                    verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    OutlinedTextField(model.feedbackSubject, { model.feedbackSubject = it }, label = { Text("Subject (optional)") }, modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(model.feedbackMessage, { model.feedbackMessage = it }, label = { Text("Feedback message") },
                        minLines = 6, modifier = Modifier.fillMaxWidth())
                    Text("Please leave out recovery phrases, private keys, and passwords.", style = MaterialTheme.typography.bodySmall)
                    Button(enabled = model.feedbackMessage.isNotBlank(), onClick = {
                        val subject = model.feedbackSubject.ifBlank { "Nockster feedback" }
                        val uri = Uri.parse("mailto:howdy@swps.io?subject=${Uri.encode(subject)}&body=${Uri.encode(model.feedbackMessage)}")
                        try { context.startActivity(Intent(Intent.ACTION_SENDTO, uri)); emailUnavailable = false }
                        catch (_: ActivityNotFoundException) { emailUnavailable = true }
                    }, modifier = Modifier.fillMaxWidth()) { Text("Open email") }
                    Text("Opens a draft to howdy@swps.io in your email app. You review and send it there.", style = MaterialTheme.typography.bodySmall)
                    if (emailUnavailable) Text("Set up an email app, or copy your message and email howdy@swps.io. Your draft is still here.", color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
}

private val NocksterLightColors = lightColorScheme(
    primary = Color.Black, onPrimary = Color.White, primaryContainer = Color(0xFFE5E7EB), onPrimaryContainer = Color(0xFF1A1A1A),
    secondary = Color(0xFF666666), onSecondary = Color.White, secondaryContainer = Color(0xFFE5E7EB), onSecondaryContainer = Color.Black,
    background = Color.White, onBackground = Color(0xFF1A1A1A), surface = Color.White, onSurface = Color(0xFF1A1A1A),
    surfaceContainer = Color(0xFFF3F4F6), surfaceContainerLow = Color(0xFFF3F4F6), surfaceContainerHigh = Color(0xFFF3F4F6),
    surfaceVariant = Color(0xFFF3F4F6), onSurfaceVariant = Color(0xFF666666), surfaceTint = Color.Transparent,
    outline = Color(0xFF999999), outlineVariant = Color(0xFFE5E7EB), error = Color(0xFFB91C1C)
)
private val NocksterDarkColors = darkColorScheme(
    primary = Color(0xFFF2F2F2), onPrimary = Color(0xFF141414), primaryContainer = Color(0xFF303030), onPrimaryContainer = Color.White,
    secondary = Color(0xFFB3B3B3), onSecondary = Color(0xFF141414), secondaryContainer = Color(0xFF303030), onSecondaryContainer = Color.White,
    background = Color(0xFF141414), onBackground = Color(0xFFE6E6E6), surface = Color(0xFF141414), onSurface = Color(0xFFE6E6E6),
    surfaceContainer = Color(0xFF1C1C1C), surfaceContainerLow = Color(0xFF1C1C1C), surfaceContainerHigh = Color(0xFF242424),
    surfaceVariant = Color(0xFF1C1C1C), onSurfaceVariant = Color(0xFFB3B3B3), surfaceTint = Color.Transparent,
    outline = Color(0xFF777777), outlineVariant = Color(0xFF303030), error = Color(0xFFFF6B6B)
)

private val NocksterBrandFont = FontFamily(Font(R.font.nokora_extra_bold, FontWeight.ExtraBold))

@Composable
private fun NocksterBrand(large: Boolean = false) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Icon(painterResource(R.drawable.nockster_mark), null, Modifier.size(if (large) 48.dp else 28.dp))
        Text("Nockster", fontFamily = NocksterBrandFont, fontWeight = FontWeight.ExtraBold,
            fontSize = if (large) 32.sp else 24.sp, letterSpacing = if (large) (-0.64).sp else (-0.48).sp)
    }
}

@Composable
private fun AddressBookScreen(model: WalletModel) {
    val scope = rememberCoroutineScope()
    var alias by remember { mutableStateOf("") }
    var address by remember { mutableStateOf("") }
    LaunchedEffect(Unit) { model.perform("contacts") }
    Text("Synced address book", style = MaterialTheme.typography.titleLarge)
    OutlinedTextField(alias, { alias = it }, label = { Text("Nickname") }, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(address, { address = it }, label = { Text("Nockchain address") }, modifier = Modifier.fillMaxWidth())
    Button(onClick = { scope.launch { if (model.perform("saveContact", JSONObject().put("name", alias).put("address", address)) != null) { alias = ""; address = "" } } }, enabled = alias.isNotBlank() && address.isNotBlank() && !model.busy) { Text("Save Address") }
    OutlinedButton(onClick = { scope.launch { model.perform("contacts") } }, enabled = !model.busy) { Text("Refresh Addresses") }
    model.snapshot.optJSONArray("contacts")?.objects().orEmpty().forEach { contact ->
        Text(contact.text("alias"), style = MaterialTheme.typography.titleMedium)
        Text(contact.text("address"), style = MaterialTheme.typography.bodySmall)
        Row {
            TextButton(onClick = { model.pendingRecipients = listOf(contact.text("address") to "") }) { Text("Use") }
            TextButton(onClick = { alias = contact.text("alias"); address = contact.text("address") }) { Text("Edit") }
            TextButton(onClick = { scope.launch { model.perform("deleteContact", JSONObject().put("contactId", contact.text("id"))) } }) { Text("Delete") }
        }
    }
}

@Composable
private fun ScanAddressDialog(model: WalletModel, onDismiss: () -> Unit, onScan: (String) -> Unit) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var granted by remember { mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) }
    var denied by remember { mutableStateOf(false) }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed ->
        model.cameraPermissionPrompt = false
        granted = allowed
        denied = !allowed
    }
    LaunchedEffect(Unit) {
        if (!granted) { model.cameraPermissionPrompt = true; permission.launch(Manifest.permission.CAMERA) }
    }
    AlertDialog(onDismissRequest = onDismiss, title = { Text("Scan address") }, text = {
        if (granted) {
            val scanner = remember { BarcodeView(context).apply {
                decoderFactory = DefaultDecoderFactory(listOf(BarcodeFormat.QR_CODE))
                decodeSingle(object : BarcodeCallback { override fun barcodeResult(result: BarcodeResult) { onScan(result.text) } })
            } }
            DisposableEffect(scanner, lifecycle) {
                val observer = LifecycleEventObserver { _, event ->
                    if (event == Lifecycle.Event.ON_RESUME) scanner.resume()
                    if (event == Lifecycle.Event.ON_PAUSE) scanner.pause()
                }
                lifecycle.addObserver(observer)
                if (lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) scanner.resume()
                onDispose { lifecycle.removeObserver(observer); scanner.pause() }
            }
            AndroidView(factory = { scanner }, modifier = Modifier.fillMaxWidth().height(320.dp))
        } else Text(if (denied) "Allow camera access in Android Settings to scan a QR code." else "Allow camera access to scan an address.")
    }, confirmButton = {
        if (denied) TextButton(onClick = { onDismiss(); context.startActivity(Intent(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${context.packageName}"))) }) { Text("Open Settings") }
    }, dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } })
}


private fun composeUsd(input: String, inNicks: Boolean, price: Double): String? {
    val number = input.trim().replace(",", "").toBigDecimalOrNull() ?: return null
    if (number.signum() < 0 || !price.isFinite() || price <= 0 || (inNicks && number.stripTrailingZeros().scale() > 0)) return null
    val nocks = if (inNicks) number.divide(java.math.BigDecimal(65536)) else number
    val value = nocks.toDouble() * price
    if (!value.isFinite()) return null
    if (value > 0 && value < 0.001) return "<0.001 USD"
    val decimals = if (value >= 1) 2 else 3
    val formatter = java.text.NumberFormat.getNumberInstance(java.util.Locale.US).apply { minimumFractionDigits = decimals; maximumFractionDigits = decimals }
    return "~${formatter.format(value)} USD"
}

@Composable
private fun UsdSubtitle(value: String?) {
    if (!value.isNullOrBlank()) Text(value, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}
