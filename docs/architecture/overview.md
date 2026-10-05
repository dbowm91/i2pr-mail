# Architecture overview

The workspace separates validated domain values (`i2pr-mail-domain`), pure MIME and protocol boundaries, durable persistence, and orchestration. Only runtime owns `MailTransport`; production implementations must supply authorized I2P byte streams. The current runtime trait is synchronous and deliberately small pending async orchestration work.

The dependency direction is domain <- mime/proto/store <- runtime. No GUI or router implementation belongs in these crates.
