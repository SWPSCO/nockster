<script lang="ts">
  import { getAddressBook, saveAddressAlias, deleteAddressAlias } from '../../utils/addressBook';
  import { onMount } from 'svelte';
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import { router } from '../../stores/router';
  import { loadFromStorage, persistToStorage } from '../../utils/storage';

  export const onAddContact: () => void = () => {};
  export let onSelectContact: (contact: any) => void = () => {};
  export let desktopLayout = false;
  export let onBack: () => void = () => {};

  let returnTo: string | null = null;
  let recipientIndex: number | null = null;
  let savedRecipients: any[] | null = null;
  let savedScrollPosition: number | null = null;

  onMount(() => {
    let mounted = true;
    const unsubscribe = router.subscribe(state => {
      returnTo = state.routeData?.returnTo || null;
      recipientIndex =
        typeof state.routeData?.recipientIndex === 'number' ? state.routeData.recipientIndex : null;
      savedRecipients = state.routeData?.recipients || null;
      savedScrollPosition =
        typeof state.routeData?.scrollPosition === 'number' ? state.routeData.scrollPosition : null;
    });

    void loadFromStorage('addressBook')
      .then(savedContacts => {
        if (mounted && savedContacts) contacts = savedContacts;
      })
      .then(() => refreshContacts())
      .catch(err => {
        error = String(err);
      });
    return () => {
      mounted = false;
      unsubscribe();
    };
  });

  function handleBack() {
    if (returnTo === 'send') {
      router.navigate('send', {
        recipients: savedRecipients,
        scrollPosition: savedScrollPosition
      });
    } else {
      onBack();
    }
  }

  function handleSelectContact(contact: any) {
    if (returnTo === 'send') {
      console.log(
        `[AddressBook] Selected contact: ${contact.name}, Address: ${contact.address}, Recipient Index: ${recipientIndex}`
      );
      // Navigate back to send with the selected address, recipient index, preserved recipients, AND scroll position
      router.navigate('send', {
        fromAddressBook: true,
        address: contact.address,
        recipientIndex: recipientIndex,
        recipients: savedRecipients,
        scrollPosition: savedScrollPosition
      });
    } else if (onSelectContact) {
      onSelectContact(contact);
    }
  }

  let searchQuery = '';
  let showAddForm = false;
  let newContact = { name: '', address: '' };
  let error = '';

  interface Contact {
    id: number | string;
    name: string;
    address: string;
    avatar: string;
  }

  let contacts: Contact[] = [];

  $: filteredContacts = contacts.filter(
    contact =>
      contact.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      contact.address.toLowerCase().includes(searchQuery.toLowerCase())
  );

  function formatAddress(address: string): string {
    if (!address || address.length <= 11) return address;
    return `${address.slice(0, 6)}...${address.slice(-4)}`;
  }

  function isValidNockchainAddress(address: string): boolean {
    // Trim whitespace first
    const trimmed = address.trim();

    if (!trimmed) return false;

    // Support both address formats:
    // 1. Bech32 format: starts with nc1, contains lowercase alphanumeric
    // 2. Base58 format: longer alphanumeric string
    const bech32Regex = /^nc1[a-z0-9]{30,}$/i;
    const base58Regex = /^[1-9A-HJ-NP-Za-km-z]{30,}$/;

    return bech32Regex.test(trimmed) || base58Regex.test(trimmed);
  }

  async function handleAddContact() {
    error = '';

    if (!newContact.name.trim()) {
      error = 'Please enter a contact name';
      return;
    }

    // Trim the address
    newContact.address = newContact.address.trim();

    if (!newContact.address) {
      error = 'Please enter a wallet address';
      return;
    }

    if (!isValidNockchainAddress(newContact.address)) {
      error = 'Invalid Nockchain address. Must be a Base58-encoded address.';
      return;
    }

    try {
      await saveAddressAlias(newContact.address, newContact.name);
      await refreshContacts();
    } catch (failure) {
      error = String(failure);
      return;
    }

    newContact = { name: '', address: '' };
    showAddForm = false;
  }

  async function refreshContacts() {
    const remote = await getAddressBook();
    const local = (await loadFromStorage('addressBook')) ?? [];
    contacts = [
      ...remote.map(contact => ({
        id: contact.id,
        name: contact.alias,
        address: contact.address,
        avatar: '👤'
      })),
      ...local.filter(
        (contact: Contact) => !remote.some(alias => alias.address === contact.address)
      )
    ];
  }

  async function handleDeleteContact(id: number | string) {
    try {
      const contact = contacts.find(contact => contact.id === id);
      if (typeof id === 'string') await deleteAddressAlias(id);
      const local = (await loadFromStorage('addressBook')) ?? [];
      await persistToStorage(
        'addressBook',
        local.filter((item: Contact) => item.address !== contact?.address)
      );
      await refreshContacts();
    } catch (failure) {
      error = String(failure);
    }
  }
</script>

<div class="address-book">
  <Header title="Address Book" showBack={true} on:click={handleBack} />

  <div class="address-content">
    <p>
      Synced with your Nockblocks account. Local contacts remain on this device until saved online.
    </p>
    <Button
      variant="secondary"
      size="small"
      on:click={() =>
        refreshContacts().catch(failure => {
          error = String(failure);
        })}>Refresh</Button
    >
    {#if error && !showAddForm}<p role="alert">{error}</p>{/if}
    <div class="search-section">
      <input
        type="text"
        class="search-input"
        placeholder="Search contacts..."
        bind:value={searchQuery}
      />
      <Button variant="primary" size="small" on:click={() => (showAddForm = true)}>
        + Add Contact
      </Button>
    </div>

    {#if showAddForm}
      <div class="add-form">
        <h3 class="form-title">Add New Contact</h3>
        <div class="form-group">
          <input
            type="text"
            class="input-field"
            placeholder="Contact Name"
            bind:value={newContact.name}
          />
        </div>
        <div class="form-group">
          <input
            type="text"
            class="input-field"
            placeholder="Nockchain address"
            bind:value={newContact.address}
          />
        </div>
        {#if error}
          <div class="error-message">{error}</div>
        {/if}
        <div class="form-actions">
          <Button
            variant="secondary"
            size="small"
            on:click={() => {
              showAddForm = false;
              newContact = { name: '', address: '' };
              error = '';
            }}
          >
            Cancel
          </Button>
          <Button variant="primary" size="small" on:click={handleAddContact}>Add Contact</Button>
        </div>
      </div>
    {/if}

    <div class="contacts-list">
      {#if filteredContacts.length === 0}
        <div class="empty-state">
          <p class="empty-text">
            {searchQuery ? 'No contacts found' : 'No contacts yet'}
          </p>
          <p class="empty-subtext">
            {searchQuery ? 'Try a different search' : 'Add your first contact to get started'}
          </p>
        </div>
      {:else}
        {#each filteredContacts as contact}
          <div class="contact-item">
            <div class="contact-avatar">{contact.avatar}</div>
            <div class="contact-info">
              <div class="contact-name">{contact.name}</div>
              <div class="contact-address">{formatAddress(contact.address)}</div>
            </div>
            <div class="contact-actions">
              <a
                href={`https://nockblocks.com/address/${contact.address}`}
                target="_blank"
                rel="noopener noreferrer"
                class="btn-icon"
                title="View on Nockblocks"
                aria-label="View on Nockblocks"
              >
                {desktopLayout ? '🌐' : '↗'}
              </a>
              <button
                class="btn-icon send"
                on:click={() => handleSelectContact(contact)}
                title="Send to this address"
              >
                →
              </button>
              <button
                class="btn-icon delete"
                on:click={() => handleDeleteContact(contact.id)}
                title="Delete contact"
              >
                ×
              </button>
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .address-book {
    height: 100%;
    background: var(--color-background);
    display: flex;
    flex-direction: column;
  }

  .address-content {
    flex: 1;
    padding: 16px;
    overflow-y: auto;
  }

  .search-section {
    display: flex;
    gap: 12px;
    margin-bottom: 20px;
  }

  .search-input {
    flex: 1;
    padding: 10px 12px;
    border: 1px solid var(--color-border) 359;
    border-radius: 8px;
    background: var(--color-surface);
    font-size: 14px;
    outline: none;
  }

  .search-input:focus {
    border-color: var(--color-text);
  }

  .add-form {
    background: var(--color-surface);
    border: 1px solid var(--color-border) 606;
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 20px;
  }

  .form-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 12px;
  }

  .form-group {
    margin-bottom: 12px;
  }

  .input-field {
    width: 100%;
    padding: 10px 12px;
    border: 1px solid var(--color-border) 926;
    border-radius: 6px;
    background: var(--color-background);
    font-size: 14px;
    outline: none;
  }

  .input-field:focus {
    border-color: var(--color-text);
  }

  .error-message {
    background: var(--color-error-light);
    border: 1px solid var(--color-error) 1212;
    color: var(--color-error);
    padding: 6px 10px;
    border-radius: 4px;
    font-size: 14px;
    margin-bottom: 12px;
  }

  .form-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .contacts-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .contact-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-border) 1629;
    border-radius: 8px;
    transition: all 0.2s;
  }

  .contact-item:hover {
    background: var(--color-border);
  }

  .contact-avatar {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--color-border);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    flex-shrink: 0;
  }

  .contact-info {
    flex: 1;
    min-width: 0;
  }

  .contact-name {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 2px;
  }

  .contact-address {
    font-size: 14px;
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .contact-actions {
    display: flex;
    gap: 4px;
  }

  .btn-icon {
    width: 32px;
    height: 32px;
    border: 1px solid var(--color-border) 2439;
    border-radius: 6px;
    background: var(--color-background);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    font-size: 16px;
    transition: all 0.2s;
    color: var(--color-text);
    text-decoration: none;
  }

  .btn-icon:hover {
    background: var(--color-surface);
    border-color: var(--color-text);
  }

  .btn-icon.send {
    color: var(--color-text);
  }

  .btn-icon.send:hover {
    background: var(--color-surface);
    border-color: var(--color-text);
  }

  .btn-icon.delete {
    color: var(--color-error);
    font-size: 20px;
  }

  .btn-icon.delete:hover {
    background: var(--color-error-light);
    border-color: var(--color-error);
  }

  .empty-state {
    text-align: center;
    padding: 40px 20px;
  }

  .empty-text {
    font-size: 16px;
    font-weight: 500;
    color: var(--color-text);
    margin-bottom: 8px;
  }

  .empty-subtext {
    font-size: 14px;
    color: var(--color-text-secondary);
  }
</style>
