# Configurar explicações com IA

## IA local do StrawberryDisk

Em **Configurações → IA**, escolha **IA local do StrawberryDisk — gratuita** e clique em **Baixar IA local**. O aplicativo baixa um motor CPU do [llama.cpp](https://github.com/ggml-org/llama.cpp) e o modelo [Qwen3-0.6B Q4](https://huggingface.co/QuantFactory/Qwen3-0.6B-GGUF), verifica os hashes SHA-256 e os guarda nos dados locais do aplicativo. O download inicial é de aproximadamente 500 MB. A instalação do aplicativo não inclui o modelo.

Depois do download, as explicações usam o modelo neste computador, sem enviar nomes ou caminhos dos itens ao serviço original. A IA é iniciada apenas quando uma explicação é solicitada. **Remover IA local** apaga o modelo e o motor baixados; o serviço personalizado permanece disponível. O StrawberryDisk mantém os prompts e o contexto dos itens para ambos os modos. Por ser pequeno, o modelo pode responder em inglês mesmo quando o idioma selecionado é português. As explicações podem estar erradas e não autorizam limpeza ou exclusão.

Para usar sua própria API, abra **Configurações → IA → Serviço personalizado**.

1. Informe a URL base da API compatível com OpenAI. Use a URL fornecida pelo seu provedor, incluindo `/v1` quando indicado.
2. Informe o identificador exato do modelo no provedor.
3. Cole a chave de API e escolha **Salvar e testar**.

## Serviço personalizado

Alguns provedores exigem parâmetros de raciocínio, cabeçalhos ou caminhos de API específicos. Abra **Configurações avançadas** e siga a documentação do provedor para esses campos. Um erro de autenticação pode indicar chave inválida, conta sem acesso ao modelo ou URL de uma região diferente da conta.

A chave de API fica nas configurações locais do aplicativo. Não a inclua em capturas de tela, relatórios ou chamados.
