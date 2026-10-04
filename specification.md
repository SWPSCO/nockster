# Nockster - Nockchain Light Wallet (This is potentially outdated)

Nockster is a Chrome extension that serves as a secure, user-friendly light wallet for the Nockchain network. Built for both beginners and advanced users, Nockster provides comprehensive wallet management while maintaining the highest security standards.

## What is Nockchain?

Nockchain is a UTXO-based blockchain where individual units of value are called "notes." Unlike traditional account-based systems, you own specific notes that you can spend to create new notes for recipients. This system provides enhanced privacy and security, with each note being individually spendable with customizable fees.

## Key Features

### 🔐 **Multi-Wallet Management**
- Store multiple independent wallets in a single, password-protected vault
- Each wallet operates with its own private key and transaction history
- Create separate wallets for different purposes (personal, business, investments)

### 🛡️ **Advanced Security**
- Local password protection with automatic locking
- Industry-standard BIP-39 seed phrase generation
- BIP-32 hierarchical deterministic address derivation
- Automatic lock after customizable timeout (default 5 minutes)
- All data stored locally on your device

### 💸 **Flexible Transaction System**
- Send to single or multiple recipients in one transaction
- Smart fee calculation based on 10-block moving average
- Option to set custom fees including zero-fee transactions during low activity
- Real-time fee recommendations (Fast, Standard, Economy)

### 📱 **User-Friendly Interface**
- Clean, intuitive design optimized for Chrome extension format
- Real-time balance and transaction history
- Network connectivity status and sync indicators
- Comprehensive error handling with clear explanations

### 🏷️ **Address Book & Privacy**
- Save frequently used addresses with custom names
- Generate new receiving addresses for enhanced privacy
- Export and import address book data

### ⚡ **Note Optimization**
- Built-in note consolidation feature
- Automatic recommendations for when to consolidate
- Visual representation of note distribution
- Optimization for future transaction efficiency

## How Nockster Works

### Understanding Notes
Nockchain operates on a "note" system where each note represents a specific amount of value that you own outright. Think of notes as digital cash denominations - you might own several notes of different values, and when you want to make a payment, you spend specific notes to create new ones for your recipients.

### The Seed Phrase - Your Master Key
When you create a wallet, Nockster generates a unique 12-24 word seed phrase using industry-standard BIP-39 protocols. This phrase is the master key to all your funds - anyone with this phrase can access your entire wallet. The seed phrase generates all your addresses using BIP-32 hierarchical deterministic protocols, meaning you can recover your entire wallet history from just these words.

**⚠️ Critical:** Write down your seed phrase and store it safely offline. Never share it digitally or with anyone.

### Multiple Wallets vs. Multiple Addresses
- **Multiple Addresses (within one wallet):** Use for privacy while keeping funds accessible through the same seed phrase. Perfect for organizing different types of incoming payments.
- **Multiple Wallets:** Use for complete separation of funds. Each wallet has its own seed phrase and operates independently. Ideal for separating personal and business funds.

### Transaction Fees and Note Management
Each note you spend incurs a separate fee, not the transaction as a whole. If you're spending 5 small notes to make a payment, you'll pay 5 individual fees. This is why consolidation matters - combining many small notes into fewer large ones reduces future transaction costs. Nockster automatically selects which notes to spend for optimal efficiency.

### Network Connectivity
Nockster connects to Nockchain network nodes to broadcast transactions and check balances. The extension displays your connection status and will clearly indicate when you're offline or when network issues occur. Your wallet data remains safe locally regardless of network status.

## User Stories

### Getting Started
- **As a new user**, I want to create my first wallet with a secure password and seed phrase so that I can safely store my Nockchain assets with the ability to recover them if needed
- **As an existing user**, I want to import my wallet using my seed phrase so that I can access my funds from any device running Nockster
- **As a security-conscious user**, I want my wallet to automatically lock after a customizable timeout so that my funds remain secure even if I forget to lock it manually

### Daily Usage
- **As a regular user**, I want to see my current balance and recent transaction history so that I can track my financial activity at a glance
- **As someone making payments**, I want to send Nockchain to one or multiple recipients with clear fee information so that I can make informed financial decisions
- **As a privacy-focused user**, I want to generate new receiving addresses for each transaction so that I can prevent others from easily tracking my transaction history

### Advanced Features
- **As an organized user**, I want to create multiple isolated wallets so that I can separate personal, business, and investment funds completely
- **As a frequent user**, I want to save commonly used addresses in an address book so that I don't have to remember or re-type long blockchain addresses
- **As an efficiency-focused user**, I want to consolidate my scattered notes so that I can reduce future transaction fees and simplify my wallet state

### Security & Management
- **As a cautious user**, I want multiple confirmation steps before deleting wallets or clearing data so that I don't accidentally lose access to my funds
- **As a user managing multiple wallets**, I want to rename my wallets with meaningful names so that I can easily identify their purpose and switch between them
- **As someone concerned about data privacy**, I want the option to completely clear all local wallet data so that I can remove all traces of my wallet usage when needed

## Screen-by-Screen Guide

### Initial Setup
**Password Creation Screen**
- Secure password input with strength indicators
- Password confirmation field
- Clear explanations of password importance and local storage

### Wallet Vault
**Main Vault Interface**
- List of all created wallets with custom names and last used dates
- "Add New Wallet" and "Import Wallet" buttons
- Individual wallet options (rename, delete) with appropriate warnings
- Application settings access
- "Clear All Data" option with multiple confirmation steps

### Wallet Creation & Import
**New Wallet Setup**
- Wallet name customization
- Seed phrase generation and display with copy functionality
- Seed phrase confirmation interface requiring user to re-enter words
- Strong warnings about backup importance and security

**Wallet Import**
- Seed phrase input with real-time word validation
- Derivation path options for advanced users
- Recovery verification system
- Clear security warnings about seed phrase handling

### Main Wallet Interface
**Dashboard**
- Current wallet name with easy switching option
- Total balance prominently displayed with note count
- Network status indicator (connected/syncing/offline)
- Quick action buttons: Send, Receive, Consolidate Notes
- Recent transaction history preview
- Navigation tabs: Dashboard, Send, Receive, History, Address Book, Settings

**Send Transaction**
- Recipient address field with address book integration
- Amount input with real-time balance validation
- "Add Recipient" functionality for multi-recipient transactions
- Fee selection with three tiers: Fast, Standard, Economy
- Clear fee breakdown showing per-note costs
- Transaction preview with total amounts and fees
- Confirmation dialog with all transaction details

**Receive**
- Current receiving address with QR code generation
- One-click address copying
- "Generate New Address" for privacy-focused users
- History of previously generated addresses
- Address derivation information for advanced users

**Transaction History**
- Chronological transaction list with status indicators
- Detailed transaction information: amounts, fees, timestamps, confirmations
- Search and filtering capabilities (date range, transaction type, amount)
- Export functionality for record keeping

### Specialized Features
**Address Book**
- Contact list with names and addresses
- Add, edit, and delete contact functionality
- Search capabilities for large address books
- Import/export options for backup and migration

**Note Consolidation**
- Visual representation of current note distribution
- Consolidation recommendations based on note count and sizes
- Preview of consolidation transaction with fee estimates
- Educational information about benefits and optimal timing
- One-click consolidation with confirmation dialog

**Settings**
- **Security Settings:** Auto-lock timeout, manual lock, password change
- **Wallet Settings:** Default fee preferences, address derivation options, notification settings
- **Network Settings:** Node connection preferences, fee calculation parameters
- **Data Management:** Export options, transaction history management, application reset

### Security States
**Locked Wallet**
- Clean, minimal password entry interface
- Clear indication of wallet locked state
- Forgot password warnings explaining potential data loss
- No access to sensitive information while locked

**Error Handling**
- Network connectivity issues with clear explanations
- Transaction failure notifications with specific reasons and solutions
- Invalid input warnings with helpful guidance
- Insufficient balance alerts with consolidation suggestions

## Security Best Practices

### Seed Phrase Management
- **Never store your seed phrase digitally** - write it down on paper
- **Store multiple copies** in different secure locations
- **Never share your seed phrase** with anyone
- **Verify your backup** by testing wallet recovery before using it for significant funds

### Wallet Security
- **Use a strong, unique password** for your local wallet vault
- **Enable auto-lock** and set an appropriate timeout for your usage patterns
- **Lock your wallet manually** when stepping away from your computer
- **Regularly review your transaction history** for any unauthorized activity

### Data Management
- **Understand the permanence** of blockchain transactions
- **Double-check recipient addresses** before sending transactions
- **Keep your browser and Nockster extension updated** for the latest security features
- **Use multiple wallets** to limit exposure of any single private key

## Getting Support

### Common Issues
- **"Transaction Failed"** - Check network connectivity and ensure sufficient balance including fees
- **"Cannot Connect to Network"** - Verify internet connection and try refreshing the extension
- **"Invalid Address"** - Confirm the recipient address is properly formatted for Nockchain
- **"Insufficient Balance"** - Consider consolidating notes or reducing transaction amount/fees

### Data Recovery
- **Lost Password:** Unfortunately, local passwords cannot be recovered. If you have your seed phrase, you can create a new wallet and import your funds
- **Lost Seed Phrase:** Seed phrases cannot be recovered. Ensure you maintain secure backups before this situation occurs
- **Corrupted Data:** Use the "Clear All Data" function and import your wallets using seed phrases

---

**⚠️ Important Disclaimer:** Nockster is a self-custody wallet, meaning you have complete control and responsibility for your funds. Always maintain secure backups of your seed phrases and never share them with anyone. The developers cannot recover lost seed phrases or passwords.

**Version:** 1.0.0 | **License:** MIT | **Network:** Nockchain Mainnet
