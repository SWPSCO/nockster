# Fletch Wallet - Svelte Component Refactor

A modular Svelte application refactor of the Fletch Wallet Chrome Extension mockup, implementing atomic design principles for better maintainability and scalability.

## Project Structure

```
src/
├── lib/
│   ├── components/
│   │   ├── atoms/          # Basic building blocks
│   │   ├── molecules/      # Composite components
│   │   └── organisms/      # Complex screen components
│   ├── stores/            # State management
│   └── styles/            # Global styles and variables
├── App.svelte             # Main application
└── main.ts               # Entry point
```

## Features

- **Modular Component Architecture**: Atomic design pattern with reusable components
- **State Management**: Svelte stores for wallet, transactions, and settings
- **Responsive Design**: Optimized for 360x600px Chrome extension viewport
- **Mock Data**: Built-in mock data generation for development
- **Complete User Flow**: Welcome, wallet creation, transactions, and settings

## Components

### Atomic Components
- `Button` - Primary, secondary, and ghost variants
- `Input` - Form inputs with validation
- `Card` - Container component
- `Icon` - SVG icon library
- `Logo` - Fletch branding

### Molecular Components
- `Header` - App header with navigation
- `TransactionItem` - Transaction display
- `WalletCard` - Wallet selector
- `FeeSelector` - Transaction fee selection
- `SeedWord` - Seed phrase word display
- `TabBar` - Bottom navigation

### Organism Components
- `WelcomeScreen` - Onboarding flow
- `SeedPhraseDisplay` - Seed phrase management
- `WalletDashboard` - Main wallet interface
- `SendTransaction` - Send flow with confirmation
- `TransactionHistory` - Transaction list with filters
- `Settings` - App settings and preferences

## Development

```bash
# Install dependencies
npm install

# Start development server
npm run dev

# Build for production
npm run build

# Preview production build
npm run preview
```

## State Management

The application uses three main stores:

- **walletStore**: Manages wallet data and authentication
- **transactionStore**: Handles transaction history
- **settingsStore**: Persists user preferences

## Design System

- **Colors**: Grayscale palette with semantic colors
- **Typography**: System fonts with Nokora for branding
- **Spacing**: Consistent spacing scale
- **Components**: Reusable and composable

## Browser Compatibility

Designed for Chrome Extension environment:
- Fixed 360x600px viewport
- Chrome 90+ recommended
- Local storage for persistence

## Next Steps

- Add real Bitcoin integration
- Implement secure key management
- Add unit tests for components
- Set up E2E testing
- Create Chrome extension manifest