# satoo-llm-skills
Collection of custom LLM skills to automate workflows of past projects. Synced from `commit-skill` configured as cursor command. 

---

## instructions 

- At project initialization, run the following script to automate cursor skills and command setup: 
```bash
curl -sSL https://raw.githubusercontent.com/SAT-oO/satoo-llm-skills/main/bootstrap.sh | bash
```

- For each custom skill, include them in a folder that ends with "`-skills`". Run the command `/commit-skill` in agent will: 
    1. update the this repo with the most recent version of skills 
    2. update the cursor skill content in the workspace 
