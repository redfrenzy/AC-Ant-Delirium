# Governança de Qualidade: Word Salad e Densidade de Informação

## O que é Word Salad?
Word Salad (Salada de Palavras) no contexto da Aesthetic Core refere-se a documentações que satisfazem as exigências sintáticas (presença de tags [WHAT], [WHY], [WHERE]) mas falham miseravelmente na entrega de valor semântico. Geralmente, são descrições circulares que utilizam os mesmos tokens do nome da função para "explicar" o que a função faz.

### Exemplo de Violação
`fn init_database() -> /// [WHAT] Initializes the database.`

Esta descrição é redundante. O nome da função já comunica que ela inicializa o banco de dados. Um desenvolvedor Elite deve fornecer o contexto arquitetural:
`/// [WHAT] Estabelece o pool de conexões com o SQLite e executa as migrações de esquema iniciais para o motor Ant Delirium.`

## Critérios de Aprovação Elite
Para que a documentação seja aprovada pela regra `rule_word_salad`, ela deve:
1. **Evitar Circularidade**: Não repetir apenas os substantivos e verbos do nome da função.
2. **Densidade Teórica**: Fornecer informações sobre *como* ou *qual estado* é alterado, não apenas *o quê* (que o código já mostra).
3. **Mínimo de Detalhe**: Frases com menos de 5 palavras substantivas são automaticamente marcadas para revisão, pois dificilmente carregam justificativas arquiteturais profundas.

## Recuperação
Se sua documentação for marcada como Word Salad, você deve reescrevê-la focando no "Contrato Arquitetural". Por que essa função existe neste ponto específico da hierarquia? Qual a consequência de sua execução para o sistema como um todo?
