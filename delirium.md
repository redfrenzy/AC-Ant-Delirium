# Protocolos de Governança Industrial

Este documento detalha os procedimentos de segurança para blindagem da infraestrutura de governança do projeto.

## 🔒 Protocolo de Blindagem (Lock)

Este comando transfere a posse dos arquivos para o usuário `root` e define permissões de leitura apenas para proprietários e grupos (644), impedindo que a IA ou qualquer processo de usuário os modifique.

```bash
sudo chown root:root ac_ant_delirium/build.rs ac_ant_delirium/delirium.toml
sudo chmod 644 ac_ant_delirium/build.rs ac_ant_delirium/delirium.toml
```

## 🔓 Protocolo de Liberação (Unlock)

Caso você precise ajustar as regras de governança ou o comportamento do build futuramente, utilize este comando para devolver a posse ao seu usuário atual:

```bash
sudo chown $USER:$USER ac_ant_delirium/build.rs ac_ant_delirium/delirium.toml
sudo chmod 664 ac_ant_delirium/build.rs ac_ant_delirium/delirium.toml
```
