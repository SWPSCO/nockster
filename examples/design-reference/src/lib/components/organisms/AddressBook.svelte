<script lang="ts">
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  
  export let onAddContact: () => void = () => {};
  export let onSelectContact: (contact: any) => void = () => {};
  export let onBack: () => void = () => {};
  
  let searchQuery = '';
  let showAddForm = false;
  let newContact = { name: '', address: '' };
  let error = '';
  
  interface Contact {
    id: number;
    name: string;
    address: string;
    avatar: string;
  }
  
  let contacts: Contact[] = [
    {
      id: 1,
      name: 'Alice Johnson',
      address: 'nc1qwx...7s8h',
      avatar: '👤'
    },
    {
      id: 2,
      name: 'Bob Smith',
      address: 'nc1qm2...9k3j',
      avatar: '👤'
    },
    {
      id: 3,
      name: 'Charlie Brown',
      address: 'nc1qp8...2m5n',
      avatar: '👤'
    },
    {
      id: 4,
      name: 'David Wilson',
      address: 'nc1qt5...4h7k',
      avatar: '👤'
    }
  ];
  
  $: filteredContacts = contacts.filter(contact =>
    contact.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    contact.address.toLowerCase().includes(searchQuery.toLowerCase())
  );
  
  function handleAddContact() {
    error = '';
    
    if (!newContact.name.trim()) {
      error = 'Please enter a contact name';
      return;
    }
    
    if (!newContact.address.trim()) {
      error = 'Please enter a wallet address';
      return;
    }
    
    if (!newContact.address.startsWith('nc1')) {
      error = 'Invalid Nockchain address';
      return;
    }
    
    const newId = Math.max(...contacts.map(c => c.id), 0) + 1;
    contacts = [
      ...contacts,
      {
        id: newId,
        name: newContact.name,
        address: newContact.address,
        avatar: '👤'
      }
    ];
    
    newContact = { name: '', address: '' };
    showAddForm = false;
  }
  
  function handleDeleteContact(id: number) {
    contacts = contacts.filter(c => c.id !== id);
  }
</script>

<div class="address-book">
  <Header title="Address Book" showBack={true} on:click={onBack} />
  
  <div class="address-content">
    <div class="search-section">
      <input
        type="text"
        class="search-input"
        placeholder="Search contacts..."
        bind:value={searchQuery}
      />
      <Button
        variant="primary"
        size="small"
        on:click={() => showAddForm = true}
      >
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
            placeholder="Wallet Address (nc1...)"
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
          <Button
            variant="primary"
            size="small"
            on:click={handleAddContact}
          >
            Add Contact
          </Button>
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
              <div class="contact-address">{contact.address}</div>
            </div>
            <div class="contact-actions">
              <button
                class="btn-icon"
                on:click={() => onSelectContact(contact)}
                title="Send to this address"
              >
                ↗
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
    border: 1px solid var(--color-border)359;
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
    border: 1px solid var(--color-border)606;
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
    border: 1px solid var(--color-border)926;
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
    border: 1px solid var(--color-error)1212;
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
    border: 1px solid var(--color-border)1629;
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
    border: 1px solid var(--color-border)2439;
    border-radius: 6px;
    background: var(--color-background);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    font-size: 16px;
    transition: all 0.2s;
  }
  
  .btn-icon:hover {
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