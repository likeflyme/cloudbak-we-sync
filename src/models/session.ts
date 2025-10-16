// Unified Session model for the app
// New fields: wx_acct_name (replaces wx_name), wx_key (replaces data_key)
// avatar replaces head_img
export interface Session {
  id: number
  name: string
  desc: string

  wx_id: string
  wx_acct_name: string // new preferred account display name
  wx_mobile: string
  wx_email: string
  wx_dir: string

  avatar: string
  online: boolean
  lastActive: string

  wx_key: string // new preferred data key
  aes_key: string
  xor_key: string

  autoSync: boolean
  syncFilters: string

  // Optional client info
  client_type?: string
  client_version?: string

  // Backward-compat legacy aliases (to be removed after full migration)
  wx_name?: string
  data_key?: string
  head_img?: string

  // Allow extension fields
  [key: string]: any
}

export type PartialSession = Partial<Session>;
